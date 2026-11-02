pub mod index;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::auth::middleware::{AdminUser, AuthUser};
use crate::auth::routes::AppError;
use crate::AppState;

pub use index::SearchIndex;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/search", get(search))
        .route("/dataview", get(dataview))
        .route("/admin/search/reindex", post(reindex))
}

#[derive(Deserialize, Default)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub title: Option<String>,
    pub identifier: Option<String>,
    pub maintenance_user: Option<String>,
    pub maintenance_group: Option<String>,
    pub space_id: Option<String>,
    pub folder: Option<String>,
    pub user: Option<String>,
    pub tag: Option<String>,
    pub property_name: Option<String>,
    pub property_value: Option<String>,
    pub has_attachment: Option<bool>,
    pub has_cover: Option<bool>,
    pub has_workflow: Option<bool>,
    pub has_tag: Option<bool>,
    pub has_property: Option<bool>,
    pub is_locked: Option<bool>,
    pub is_draft: Option<bool>,
    pub created_on: Option<String>,
    pub updated_on: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct DataviewQueryParams {
    pub q: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Serialize, Clone)]
pub struct SearchResult {
    pub document_id: String,
    pub human_identifier: Option<String>,
    pub maintenance_user_id: Option<String>,
    pub maintenance_group_id: Option<String>,
    pub title: String,
    pub space_id: String,
    pub space_name: String,
    pub excerpt: String,
    pub updated_at: String,
    pub author: String,
    pub author_id: String,
    pub folder_id: Option<String>,
    pub folder_name: Option<String>,
    pub is_draft: bool,
    pub draft_id: Option<String>,
}

#[derive(Clone)]
struct DataviewDocument {
    id: String,
    title: String,
    human_identifier: Option<String>,
    maintenance_user_id: Option<String>,
    maintenance_group_id: Option<String>,
    space_id: String,
    space_name: String,
    content: String,
    tags: Vec<String>,
    properties: Vec<(String, String)>,
    created_at: String,
    updated_at: String,
    author: String,
    author_id: String,
    folder_id: Option<String>,
    folder_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DataviewToken {
    Word(String),
    Quoted(String),
    Colon,
    Equals,
    Tilde,
    LParen,
    RParen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DataviewOperator {
    Contains,
    Equals,
    Fuzzy,
    Is,
    IsNot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DataviewExpression {
    Predicate {
        field: Option<String>,
        operator: DataviewOperator,
        value: String,
    },
    Not(Box<DataviewExpression>),
    And(Box<DataviewExpression>, Box<DataviewExpression>),
    Or(Box<DataviewExpression>, Box<DataviewExpression>),
}

const SEARCH_BATCH: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow)]
struct PublishedRow {
    id: String,
    title: String,
    human_identifier: Option<String>,
    maintenance_user_id: Option<String>,
    maintenance_group_id: Option<String>,
    space_id: String,
    space_name: String,
    author: String,
    author_id: String,
    author_email: String,
    updated_at: String,
    created_at: String,
    content_path: String,
    published_at: String,
    folder_id: Option<String>,
    folder_name: Option<String>,
    icon: Option<String>,
    locked: i64,
    is_public: i64,
}
#[derive(Clone)]
struct CurrentPublished {
    grant_only: bool,
    grant: Option<crate::spaces::page_viewer_grants::DirectViewerGrant>,
    allow_grants: bool,
    body_loaded: bool,
    proofs: Vec<crate::spaces::published_source::SourceProof>,
    row: PublishedRow,
    content: String,
    tags: Vec<String>,
    properties: Vec<(String, String)>,
}
async fn published_row(
    conn: &mut sqlx::AnyConnection,
    id: &str,
) -> Result<Option<PublishedRow>, AppError> {
    sqlx::query_as("SELECT d.id,d.title,d.human_identifier,d.maintenance_user_id,d.maintenance_group_id,d.space_id,s.name AS space_name,COALESCE(NULLIF(u.display_name,''),u.email,'Unknown user') AS author,COALESCE(d.last_updated_by,'') AS author_id,COALESCE(u.email,'') AS author_email,d.updated_at,d.created_at,d.content_path,d.published_at,d.folder_id,f.name AS folder_name,d.icon,d.locked,d.public AS is_public FROM documents d JOIN spaces s ON s.id=d.space_id LEFT JOIN users u ON u.id=d.last_updated_by LEFT JOIN folders f ON f.id=d.folder_id AND f.space_id=d.space_id WHERE d.id=$1 AND d.published_at IS NOT NULL AND d.archived_at IS NULL AND d.deleted_at IS NULL").bind(id).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn page_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
) -> Result<(), AppError> {
    crate::spaces::access::resolve_page_access(
        conn,
        crate::spaces::access::principal_from_auth(auth)?,
        space,
        id,
        crate::spaces::access::PageOperation::View,
        crate::spaces::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}
fn unavailable(error: &AppError) -> bool {
    matches!(
        error,
        AppError::NotFound(_) | AppError::Forbidden(_) | AppError::Conflict(_)
    )
}
async fn scoped_metadata(
    conn: &mut sqlx::AnyConnection,
    row: &PublishedRow,
) -> Result<(Vec<String>, Vec<(String, String)>), AppError> {
    let tags:Vec<(String,)>=sqlx::query_as("SELECT t.name FROM tags t JOIN doc_tags dt ON dt.tag_id=t.id WHERE dt.document_id=$1 AND t.space_id=$2 ORDER BY t.id").bind(&row.id).bind(&row.space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let properties=sqlx::query_as("SELECT ps.name,COALESCE(pv.value,'') FROM property_values pv JOIN properties_schema ps ON ps.id=pv.property_id WHERE pv.document_id=$1 AND ps.space_id=$2 ORDER BY ps.id").bind(&row.id).bind(&row.space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    Ok((tags.into_iter().map(|(name,)| name).collect(), properties))
}
async fn load_published(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    id: &str,
    requested_space: Option<&str>,
    body: bool,
    allow_grants: bool,
) -> Result<Option<CurrentPublished>, AppError> {
    let Some(row) = published_row(conn, id).await? else {
        return Ok(None);
    };
    if requested_space.is_some_and(|space| space != row.space_id) {
        return Ok(None);
    }
    let page=match crate::spaces::access::resolve_page_access(conn,crate::spaces::access::principal_from_auth(auth)?,&row.space_id,&row.id,if allow_grants {crate::spaces::access::PageOperation::GrantedPublishedView} else {crate::spaces::access::PageOperation::View},crate::spaces::access::Purpose::Routine).await {
        Ok(page)=>page,Err(e) if unavailable(&e)=>return Ok(None),Err(e)=>return Err(e),
    };
    let grant_only=page.direct_viewer_grant.is_some();
    let proofs=crate::spaces::published_source::capture_source(conn,auth,&row.space_id,&row.id,&page.ancestor_ids).await?;
    let (tags, properties) = scoped_metadata(conn, &row).await?;
    // Capture the original served bytes; current_published repeats exact SQL,
    // raw audience and named page admission after I/O. Final response delivery
    // also compares these same bytes synchronously after original Read.
    let content = if body {
        match crate::storage::read_document(&row.content_path).await {
            Ok(content) => content,
            Err(_) => return Ok(None),
        }
    } else {
        String::new()
    };
    let source = CurrentPublished {
        grant_only,grant:page.direct_viewer_grant,allow_grants,body_loaded:body,proofs,
        row,
        content,
        tags,
        properties,
    };
    #[cfg(test)]
    crate::spaces::shared_pages::reader_wait().await;
    if !current_published(conn, auth, &source).await? {
        return Ok(None);
    }
    Ok(Some(source))
}
async fn current_published(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    source: &CurrentPublished,
) -> Result<bool, AppError> {
    if published_row(conn,&source.row.id).await?.as_ref()!=Some(&source.row) {return Ok(false);}
    let (tags,properties)=scoped_metadata(conn,&source.row).await?;
    if tags!=source.tags || properties!=source.properties {return Ok(false);}
    if source.body_loaded && crate::storage::read_document(&source.row.content_path).await.ok().as_deref()!=Some(source.content.as_str()) {return Ok(false);}
    match crate::spaces::published_source::verify_source(conn,&source.proofs).await {
        Ok(())=>{},Err(e) if unavailable(&e)=>return Ok(false),Err(e)=>return Err(e),
    }
    match crate::spaces::access::resolve_page_access(conn,crate::spaces::access::principal_from_auth(auth)?,&source.row.space_id,&source.row.id,if source.allow_grants {crate::spaces::access::PageOperation::GrantedPublishedView} else {crate::spaces::access::PageOperation::View},crate::spaces::access::Purpose::Routine).await {
        Ok(page)=>Ok(page.direct_viewer_grant==source.grant),Err(e) if unavailable(&e)=>Ok(false),Err(e)=>Err(e),
    }
}
// These filters reference catalogs or identities not granted by a page-only
// viewer. Reject both positive and negative predicates, rather than substituting
// blank fields and accidentally making hidden metadata queryable through NOT.
fn grant_filters_allowed(q:&SearchQuery)->bool {
    !q.q.as_deref().unwrap_or_default().contains(':')
        && q.space_id.is_none() && q.folder.is_none() && q.user.is_none() && q.tag.is_none()
        && q.property_name.is_none() && q.property_value.is_none()
        && q.maintenance_user.is_none() && q.maintenance_group.is_none()
        && q.has_attachment.is_none() && q.has_workflow.is_none()
        && q.has_tag.is_none() && q.has_property.is_none()
}
async fn published_filters(conn:&mut sqlx::AnyConnection,auth:&AuthUser,source:&CurrentPublished,q:&SearchQuery)->Result<bool,AppError> {
    if source.grant_only {
        if !grant_filters_allowed(q) {return Ok(false);}
        // Tantivy's ordinary default includes tags. Page-only search must prove
        // the actual published title/body matched, rather than expose hidden
        // tags by their presence in an indexed hit. This first cut supports
        // literal text (and quoted literal text), not catalog/query syntax.
        let needle=q.q.as_deref().unwrap_or_default().trim().trim_matches('"').to_lowercase();
        if !needle.is_empty() && !source.row.title.to_lowercase().contains(&needle) && !source.content.to_lowercase().contains(&needle) {return Ok(false);}
    }
    current_filters(conn,auth,&source.row,q).await
}
fn indexed_source_matches(hit: &index::SearchCandidate, source: &CurrentPublished) -> bool {
    if hit.title != source.row.title || hit.space_id != source.row.space_id {
        return false;
    }
    // Writers index either the complete mirror or publish's pre-mirror portable
    // content. Recompose the latter using the SAME storage composition rules;
    // never strip an arbitrary old header and manufacture a matching source.
    let frontmatter = crate::spaces::documents::document_storage_frontmatter(
        &source.row.title,
        source.row.icon.as_deref(),
        source.row.locked != 0,
        source.row.is_public != 0,
    );
    let normalized = hit.body.replace("\r\n", "\n");
    let composed = if let Some((metadata, body)) = normalized
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
    {
        format!(
            "---\n{}\n{}\n---\n\n{}",
            frontmatter.trim(),
            metadata.trim(),
            body.trim_start()
        )
    } else {
        format!("---\n{}\n---\n\n{}", frontmatter.trim(), normalized)
    };
    if hit.body != source.content && composed != source.content {
        return false;
    }
    if source.grant_only {return true;}
    // Empty tags are the normal page/publish writer representation. A populated
    // reindex tag source must still match current scoped names, irrespective of
    // the historical SQL aggregation order.
    if !hit.tags.is_empty() {
        let mut old: Vec<_> = hit.tags.split_whitespace().collect();
        let names = source.tags.join(" ");
        let mut current: Vec<_> = names.split_whitespace().collect();
        old.sort_unstable();
        current.sort_unstable();
        if old != current {
            return false;
        }
    }
    true
}
fn result_from_published(source: &CurrentPublished, excerpt: Option<usize>) -> SearchResult {
    let row = &source.row;
    SearchResult {
        document_id: row.id.clone(),
        title: row.title.clone(),
        human_identifier: row.human_identifier.clone(),
        maintenance_user_id: if source.grant_only {None} else {row.maintenance_user_id.clone()},
        maintenance_group_id: if source.grant_only {None} else {row.maintenance_group_id.clone()},
        space_id: row.space_id.clone(),
        space_name: if source.grant_only {String::new()} else {row.space_name.clone()},
        excerpt: match excerpt {
            Some(200) => index::excerpt(&source.content, 200),
            Some(limit) => source
                .content
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(limit)
                .collect(),
            None => String::new(),
        },
        updated_at: row.updated_at.clone(),
        author: if source.grant_only {String::new()} else {row.author.clone()},
        author_id: if source.grant_only {String::new()} else {row.author_id.clone()},
        folder_id: if source.grant_only {None} else {row.folder_id.clone()},
        folder_name: if source.grant_only {None} else {row.folder_name.clone()},
        is_draft: false,
        draft_id: None,
    }
}
fn filter_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}
fn maintenance_matches(user: Option<&str>, group: Option<&str>, query: &SearchQuery) -> bool {
    query
        .maintenance_user
        .as_deref()
        .filter(|s| !s.is_empty())
        .is_none_or(|expected| user == Some(expected))
        && query
            .maintenance_group
            .as_deref()
            .filter(|s| !s.is_empty())
            .is_none_or(|expected| group == Some(expected))
}
fn identifier_matches(actual: Option<&str>, filter: Option<&str>) -> bool {
    filter_text(filter)
        .is_none_or(|expected| actual.is_some_and(|value| value.eq_ignore_ascii_case(expected)))
}
async fn current_filters(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    row: &PublishedRow,
    q: &SearchQuery,
) -> Result<bool, AppError> {
    if !maintenance_matches(
        row.maintenance_user_id.as_deref(),
        row.maintenance_group_id.as_deref(),
        q,
    ) {
        return Ok(false);
    }
    if !identifier_matches(row.human_identifier.as_deref(), q.identifier.as_deref()) {
        return Ok(false);
    }
    let found:Option<i64>=sqlx::query_scalar("SELECT 1 FROM documents d LEFT JOIN users u ON u.id=d.last_updated_by LEFT JOIN folders f ON f.id=d.folder_id AND f.space_id=d.space_id WHERE d.id=$1 AND d.space_id=$2 AND ($3 IS NULL OR lower(d.title) LIKE lower($3)) AND ($4 IS NULL OR f.id=$4 OR lower(f.name)=lower($4)) AND ($5 IS NULL OR u.id=$5 OR lower(u.display_name)=lower($5) OR lower(u.email)=lower($5)) AND ($6 IS NULL OR EXISTS(SELECT 1 FROM doc_tags dt JOIN tags t ON t.id=dt.tag_id WHERE dt.document_id=d.id AND t.space_id=d.space_id AND lower(t.name)=lower($6))) AND (($7 IS NULL AND $8 IS NULL) OR EXISTS(SELECT 1 FROM property_values pv JOIN properties_schema ps ON ps.id=pv.property_id WHERE pv.document_id=d.id AND ps.space_id=d.space_id AND ($7 IS NULL OR lower(ps.name)=lower($7)) AND ($8 IS NULL OR lower(COALESCE(pv.value,'')) LIKE lower($8)))) AND ($9 IS NULL OR CASE WHEN d.cover_image IS NOT NULL AND trim(d.cover_image)!='' THEN 1 ELSE 0 END=$9) AND ($10 IS NULL OR CASE WHEN EXISTS(SELECT 1 FROM document_workflows dw JOIN workflow_templates wt ON wt.id=dw.workflow_template_id WHERE dw.document_id=d.id AND wt.space_id=d.space_id) THEN 1 ELSE 0 END=$10) AND ($11 IS NULL OR CASE WHEN EXISTS(SELECT 1 FROM doc_tags dt JOIN tags t ON t.id=dt.tag_id WHERE dt.document_id=d.id AND t.space_id=d.space_id) THEN 1 ELSE 0 END=$11) AND ($12 IS NULL OR CASE WHEN EXISTS(SELECT 1 FROM property_values pv JOIN properties_schema ps ON ps.id=pv.property_id WHERE pv.document_id=d.id AND ps.space_id=d.space_id) THEN 1 ELSE 0 END=$12) AND ($13 IS NULL OR d.locked=$13) AND ($14 IS NULL OR substr(d.created_at,1,10)=$14) AND ($15 IS NULL OR substr(d.updated_at,1,10)=$15)")
    .bind(&row.id).bind(&row.space_id).bind(filter_text(q.title.as_deref()).map(|s|format!("%{s}%"))).bind(filter_text(q.folder.as_deref())).bind(filter_text(q.user.as_deref())).bind(filter_text(q.tag.as_deref())).bind(filter_text(q.property_name.as_deref())).bind(filter_text(q.property_value.as_deref()).map(|s|format!("%{s}%"))).bind(q.has_cover.map(i64::from)).bind(q.has_workflow.map(i64::from)).bind(q.has_tag.map(i64::from)).bind(q.has_property.map(i64::from)).bind(q.is_locked.map(i64::from)).bind(q.created_on.as_deref().filter(|s|is_valid_search_date(Some(s)))).bind(q.updated_on.as_deref().filter(|s|is_valid_search_date(Some(s)))).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if found.is_none() {return Ok(false)}
    if let Some(expected)=q.has_attachment {
        let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM attachments WHERE document_id=$1 AND space_id=$2 ORDER BY id LIMIT 100001")
            .bind(&row.id).bind(&row.space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
        if ids.len()>100000 {return Err(AppError::Conflict("Attachment filter inventory exceeds its inspection bound".into()))}
        let mut visible=None;
        for id in &ids {
            match crate::spaces::asset_audiences::read(conn,auth,&row.space_id,&row.id,id).await {
                Ok(proof)=>{visible=Some((id,proof));break},
                Err(AppError::NotFound(_)|AppError::Forbidden(_))=>(),
                Err(error)=>return Err(error),
            }
        }
        let fresh:Vec<String>=sqlx::query_scalar("SELECT id FROM attachments WHERE document_id=$1 AND space_id=$2 ORDER BY id LIMIT 100001")
            .bind(&row.id).bind(&row.space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
        if fresh!=ids {return Err(AppError::Conflict("Attachment filter source changed".into()))}
        if let Some((id,proof))=&visible {
            crate::spaces::asset_audiences::verify_read(conn,auth,&row.space_id,&row.id,id,proof).await?;
        }
        if visible.is_some()!=expected {return Ok(false)}
    }
    Ok(true)
}
async fn candidate_page(
    conn: &mut sqlx::AnyConnection,
    space: Option<&str>,
    cursor: Option<&(String, String)>,
) -> Result<Vec<(String, String)>, AppError> {
    sqlx::query_as("SELECT id,updated_at FROM documents WHERE published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL AND ($1 IS NULL OR space_id=$1) AND ($2 IS NULL OR updated_at<$2 OR (updated_at=$2 AND id>$3)) ORDER BY updated_at DESC,id ASC LIMIT $4").bind(space).bind(cursor.map(|(_,updated)|updated.as_str())).bind(cursor.map(|(id,_)|id.as_str())).bind(SEARCH_BATCH as i64).fetch_all(conn).await.map_err(AppError::internal)
}
async fn requested_space_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: Option<&str>,
) -> Result<(), AppError> {
    crate::spaces::access::require_current_human_read(conn, auth).await?;
    if let Some(space) = space {
        let access = crate::spaces::access::resolve_space_access(
            conn,
            crate::spaces::access::principal_from_auth(auth)?,
            space,
            crate::spaces::access::Purpose::Routine,
        )
        .await?;
        if !access.allows(crate::spaces::access::Permission::View) {
            return Err(AppError::NotFound("Target unavailable".into()));
        }
    }
    Ok(())
}
pub(crate) async fn search_visible_pages(
    state: &AppState,
    auth: &AuthUser,
    query: &SearchQuery,
) -> Result<Vec<SearchResult>, AppError> {
    let q = query.q.as_deref().unwrap_or_default().trim();
    if q.len() > 500 {
        return Err(AppError::BadRequest(
            "Search query must be at most 500 characters".into(),
        ));
    }
    let limit = query.limit.unwrap_or(20).min(100);
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    requested_space_view(&mut conn, auth, query.space_id.as_deref()).await?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    if query.is_draft == Some(true) {
        return search_drafts_current(&mut conn, auth, query, q, limit).await;
    }
    let snapshot = if q.is_empty() {
        None
    } else {
        Some(state.search_index.snapshot(q).map_err(AppError::internal)?)
    };
    let mut offset = 0;
    let mut cursor = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut results: Vec<CurrentPublished> = Vec::new();
    loop {
        let indexed = if let Some(snapshot) = &snapshot {
            Some(
                snapshot
                    .page(offset, SEARCH_BATCH)
                    .map_err(AppError::internal)?,
            )
        } else {
            None
        };
        let ids: Vec<String> = if let Some(hits) = &indexed {
            hits.iter().map(|hit| hit.doc_id.clone()).collect()
        } else {
            let rows =
                candidate_page(&mut conn, query.space_id.as_deref(), cursor.as_ref()).await?;
            cursor = rows.last().cloned();
            rows.into_iter().map(|(id, _)| id).collect()
        };
        let count = ids.len();
        if count == 0 {
            break;
        }
        offset += count;
        for (position, id) in ids.into_iter().enumerate() {
            if seen.contains(&id) {
                continue;
            }
            let Some(source) = load_published(
                &mut conn,
                auth,
                &id,
                query.space_id.as_deref(),
                snapshot.is_some(),
                true,
            )
            .await?
            else {
                continue;
            };
            if !published_filters(&mut conn, auth, &source, query).await? {
                continue;
            }
            if indexed
                .as_ref()
                .is_some_and(|hits| !indexed_source_matches(&hits[position], &source))
            {
                continue;
            }
            seen.insert(id);
            results.push(source);
        }
        #[cfg(test)]
        tests::pause_search_read().await;
        let mut current = Vec::new();
        for source in results {
            if published_filters(&mut conn, auth, &source, query).await?
                && current_published(&mut conn, auth, &source).await?
            {
                current.push(source)
            }
        }
        results = current;
        requested_space_view(&mut conn, auth, query.space_id.as_deref()).await?;
        if results.len() >= limit || count < SEARCH_BATCH {
            break;
        }
    }
    #[cfg(test)]
    tests::pause_search_final().await;
    requested_space_view(&mut conn, auth, query.space_id.as_deref()).await?;
    let mut final_results = Vec::new();
    for source in results {
        #[cfg(test)]
        crate::spaces::shared_pages::policy_wait(&source.row.id).await;
        if published_filters(&mut conn, auth, &source, query).await?
            && current_published(&mut conn, auth, &source).await?
        {
            final_results.push(source);
        }
    }
    let mut witnessed=Vec::new();
    for source in final_results {
        if published_row(&mut conn,&source.row.id).await?.as_ref()!=Some(&source.row) {continue;}
        match crate::spaces::published_source::verify_source(&mut conn,&source.proofs).await {
            Ok(())=>{},Err(e) if unavailable(&e)=>continue,Err(e)=>return Err(e),
        }
        if let Some(grant)=source.grant.as_ref() {
            if crate::spaces::page_viewer_grants::read_current_direct_viewer_grant(&mut conn,&source.row.space_id,&source.row.id,&auth.claims.sub).await?.as_ref()!=Some(grant) {continue;}
        }
        witnessed.push(source);
    }
    let mut final_results=witnessed;
    crate::spaces::access::require_current_human_read(&mut conn,auth).await?;
    final_results.retain(|source|!source.body_loaded || std::fs::read(crate::storage::data_dir().join(&source.row.content_path)).ok().as_deref()==Some(source.content.as_bytes()));
    Ok(final_results
        .iter()
        .take(limit)
        .map(|source| result_from_published(source, snapshot.as_ref().map(|_| 200)))
        .collect())
}
async fn search(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(search_visible_pages(&state, &auth, &query).await?))
}

#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow)]
struct DraftSearchRow {
    id: String,
    document_id: String,
    space_id: String,
    owner_id: String,
    title: String,
    human_identifier: Option<String>,
    maintenance_user_id: Option<String>,
    maintenance_group_id: Option<String>,
    content: String,
    updated_at: String,
    base_updated_at: String,
    space_name: String,
    author: String,
    author_email: String,
}
async fn draft_row(
    conn: &mut sqlx::AnyConnection,
    id: &str,
) -> Result<Option<DraftSearchRow>, AppError> {
    sqlx::query_as("SELECT dd.id,dd.document_id,dd.space_id,dd.owner_id,dd.title,d.human_identifier,d.maintenance_user_id,d.maintenance_group_id,dd.content,dd.updated_at,dd.base_updated_at,s.name AS space_name,COALESCE(NULLIF(u.display_name,''),u.email,'Unknown user') AS author,COALESCE(u.email,'') AS author_email FROM document_drafts dd JOIN documents d ON d.id=dd.document_id AND d.space_id=dd.space_id JOIN spaces s ON s.id=dd.space_id LEFT JOIN users u ON u.id=dd.owner_id WHERE dd.id=$1").bind(id).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn current_draft(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    row: &DraftSearchRow,
) -> Result<bool, AppError> {
    match crate::spaces::documents::require_discovery_draft_view(
        conn,
        auth,
        &row.space_id,
        &row.document_id,
        &row.id,
    )
    .await
    {
        Ok(()) => {}
        Err(e) if unavailable(&e) => return Ok(false),
        Err(e) => return Err(e),
    }
    Ok(draft_row(conn, &row.id).await?.as_ref() == Some(row))
}
async fn draft_filters_match(
    conn: &mut sqlx::AnyConnection,
    row: &DraftSearchRow,
    query: &SearchQuery,
    q: &str,
) -> Result<bool, AppError> {
    if !maintenance_matches(
        row.maintenance_user_id.as_deref(),
        row.maintenance_group_id.as_deref(),
        query,
    ) {
        return Ok(false);
    }
    if !identifier_matches(row.human_identifier.as_deref(), query.identifier.as_deref()) {
        return Ok(false);
    }
    let found:Option<i64>=sqlx::query_scalar("SELECT 1 FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3 AND ($4='' OR lower(title) LIKE lower($5) OR lower(content) LIKE lower($5)) AND ($6 IS NULL OR lower(title) LIKE lower($6)) AND ($7 IS NULL OR substr(updated_at,1,10)=$7)").bind(&row.id).bind(&row.document_id).bind(&row.space_id).bind(q).bind(format!("%{q}%")).bind(filter_text(query.title.as_deref()).map(|s|format!("%{s}%"))).bind(query.updated_on.as_deref().filter(|s|is_valid_search_date(Some(s)))).fetch_optional(conn).await.map_err(AppError::internal)?;
    Ok(found.is_some())
}
async fn search_drafts_current(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    query: &SearchQuery,
    q: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, AppError> {
    let actor = crate::spaces::access::require_current_human_read(conn, auth).await?;
    let mut cursor: Option<(String, String)> = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut results = Vec::new();
    loop {
        let candidates:Vec<(String,String)>=sqlx::query_as("SELECT dd.id,dd.updated_at FROM document_drafts dd WHERE (dd.owner_id=$1 OR EXISTS(SELECT 1 FROM document_draft_access dda WHERE dda.draft_id=dd.id AND dda.user_id=$1 AND dda.role IN ('viewer','editor'))) AND ($2 IS NULL OR dd.space_id=$2) AND ($3='' OR lower(dd.title) LIKE lower($4) OR lower(dd.content) LIKE lower($4)) AND ($5 IS NULL OR lower(dd.title) LIKE lower($5)) AND ($6 IS NULL OR substr(dd.updated_at,1,10)=$6) AND ($7 IS NULL OR dd.updated_at<$7 OR (dd.updated_at=$7 AND dd.id>$8)) ORDER BY dd.updated_at DESC,dd.id ASC LIMIT $9")
        .bind(&actor).bind(query.space_id.as_deref()).bind(q).bind(format!("%{q}%")).bind(filter_text(query.title.as_deref()).map(|s|format!("%{s}%"))).bind(query.updated_on.as_deref().filter(|s|is_valid_search_date(Some(s)))).bind(cursor.as_ref().map(|(_,updated)|updated.as_str())).bind(cursor.as_ref().map(|(id,_)|id.as_str())).bind(SEARCH_BATCH as i64).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
        let count = candidates.len();
        cursor = candidates.last().cloned();
        if count == 0 {
            break;
        }
        for (id, _) in candidates {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(row) = draft_row(conn, &id).await? else {
                continue;
            };
            if query
                .space_id
                .as_deref()
                .is_some_and(|space| space != row.space_id)
            {
                continue;
            }
            if filter_text(query.user.as_deref()).is_some_and(|user| {
                !row.owner_id.eq_ignore_ascii_case(user)
                    && !row.author.eq_ignore_ascii_case(user)
                    && !row.author_email.eq_ignore_ascii_case(user)
            }) {
                continue;
            }
            if current_draft(conn, auth, &row).await?
                && draft_filters_match(conn, &row, query, q).await?
            {
                results.push(row)
            }
        }
        #[cfg(test)]
        tests::pause_search_read().await;
        let mut current = Vec::new();
        for row in results {
            if current_draft(conn, auth, &row).await?
                && draft_filters_match(conn, &row, query, q).await?
            {
                current.push(row)
            }
        }
        results = current;
        requested_space_view(conn, auth, query.space_id.as_deref()).await?;
        if results.len() >= limit || count < SEARCH_BATCH {
            break;
        }
    }
    #[cfg(test)]
    tests::pause_search_final().await;
    requested_space_view(conn, auth, query.space_id.as_deref()).await?;
    let mut final_results = Vec::new();
    for row in results {
        if current_draft(conn, auth, &row).await?
            && draft_filters_match(conn, &row, query, q).await?
        {
            final_results.push(row);
        }
    }
    Ok(final_results
        .into_iter()
        .take(limit)
        .map(|row| SearchResult {
            document_id: row.document_id,
            title: row.title,
            human_identifier: row.human_identifier,
            maintenance_user_id: row.maintenance_user_id,
            maintenance_group_id: row.maintenance_group_id,
            space_id: row.space_id,
            space_name: row.space_name,
            excerpt: row
                .content
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(180)
                .collect(),
            updated_at: row.updated_at,
            author: row.author,
            author_id: row.owner_id,
            folder_id: None,
            folder_name: None,
            is_draft: true,
            draft_id: Some(row.id),
        })
        .collect())
}

/// GET /api/v1/dataview?q=tag:roadmap&limit=50
async fn dataview(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<DataviewQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    let q = query.q.unwrap_or_default().trim().to_string();
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    crate::spaces::access::require_current_human_read(&mut conn, &auth).await?;
    if q.len() > 500 {
        return Err(AppError::BadRequest(
            "Dataview query must be at most 500 characters".into(),
        ));
    }
    let limit = dataview_result_limit(query.limit);
    if q.is_empty() || limit == 0 {
        return Ok(Json(Vec::<SearchResult>::new()));
    }
    let expression = parse_dataview_query(&q);
    let mut cursor = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut results = Vec::new();
    loop {
        let candidates = candidate_page(&mut conn, None, cursor.as_ref()).await?;
        let count = candidates.len();
        cursor = candidates.last().cloned();
        if count == 0 {
            break;
        }
        for (id, _) in candidates {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(source) = load_published(&mut conn, &auth, &id, None, true, false).await? else {
                continue;
            };
            let row = &source.row;
            let document = DataviewDocument {
                id: row.id.clone(),
                title: row.title.clone(),
                human_identifier: row.human_identifier.clone(),
                maintenance_user_id: row.maintenance_user_id.clone(),
                maintenance_group_id: row.maintenance_group_id.clone(),
                space_id: row.space_id.clone(),
                space_name: row.space_name.clone(),
                content: source.content.clone(),
                tags: source.tags.clone(),
                properties: source.properties.clone(),
                created_at: row.created_at.clone(),
                updated_at: row.updated_at.clone(),
                author: row.author.clone(),
                author_id: row.author_id.clone(),
                folder_id: row.folder_id.clone(),
                folder_name: row.folder_name.clone(),
            };
            if expression
                .as_ref()
                .is_none_or(|expr| evaluate_dataview_expression(&document, expr))
            {
                results.push(source)
            }
        }
        #[cfg(test)]
        tests::pause_search_read().await;
        let mut current = Vec::new();
        for source in results {
            if current_published(&mut conn, &auth, &source).await? {
                current.push(source)
            }
        }
        results = current;
        crate::spaces::access::require_current_human_read(&mut conn, &auth).await?;
        if results.len() >= limit || count < SEARCH_BATCH {
            break;
        }
    }
    #[cfg(test)]
    tests::pause_search_final().await;
    crate::spaces::access::require_current_human_read(&mut conn, &auth).await?;
    let mut final_results = Vec::new();
    for source in results {
        if current_published(&mut conn, &auth, &source).await? {
            final_results.push(source);
        }
    }
    Ok(Json(
        final_results
            .iter()
            .take(limit)
            .map(|source| result_from_published(source, Some(180)))
            .collect::<Vec<_>>(),
    ))
}

fn parse_dataview_query(source: &str) -> Option<DataviewExpression> {
    let tokens = tokenize_dataview(source);
    let mut parser = DataviewParser::new(tokens);
    parser.parse_expression()
}

fn dataview_result_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(50).min(100)
}

fn tokenize_dataview(source: &str) -> Vec<DataviewToken> {
    let chars = source.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }
        match ch {
            '"' => {
                index += 1;
                let mut value = String::new();
                while index < chars.len() {
                    if chars[index] == '\\' && index + 1 < chars.len() {
                        value.push(chars[index + 1]);
                        index += 2;
                        continue;
                    }
                    if chars[index] == '"' {
                        break;
                    }
                    value.push(chars[index]);
                    index += 1;
                }
                if index < chars.len() && chars[index] == '"' {
                    index += 1;
                }
                tokens.push(DataviewToken::Quoted(value));
            }
            ':' => {
                tokens.push(DataviewToken::Colon);
                index += 1;
            }
            '=' => {
                tokens.push(DataviewToken::Equals);
                index += 1;
            }
            '~' => {
                tokens.push(DataviewToken::Tilde);
                index += 1;
            }
            '(' => {
                tokens.push(DataviewToken::LParen);
                index += 1;
            }
            ')' => {
                tokens.push(DataviewToken::RParen);
                index += 1;
            }
            _ => {
                let mut value = String::new();
                while index < chars.len()
                    && !chars[index].is_whitespace()
                    && !matches!(chars[index], ':' | '=' | '~' | '(' | ')')
                {
                    value.push(chars[index]);
                    index += 1;
                }
                tokens.push(DataviewToken::Word(value));
            }
        }
    }
    tokens
}

struct DataviewParser {
    tokens: Vec<DataviewToken>,
    index: usize,
}

impl DataviewParser {
    fn new(tokens: Vec<DataviewToken>) -> Self {
        Self { tokens, index: 0 }
    }

    fn parse_expression(&mut self) -> Option<DataviewExpression> {
        if self.tokens.is_empty() {
            return None;
        }
        Some(self.parse_or())
    }

    fn parse_or(&mut self) -> DataviewExpression {
        let mut expression = self.parse_and();
        while self.match_word("or") {
            expression = DataviewExpression::Or(Box::new(expression), Box::new(self.parse_and()));
        }
        expression
    }

    fn parse_and(&mut self) -> DataviewExpression {
        let mut expression = self.parse_unary();
        while self.has_more() && !self.peek_rparen() && !self.peek_word("or") {
            if self.peek_word("and") {
                self.index += 1;
            }
            expression =
                DataviewExpression::And(Box::new(expression), Box::new(self.parse_unary()));
        }
        expression
    }

    fn parse_unary(&mut self) -> DataviewExpression {
        if self.match_word("not") {
            return DataviewExpression::Not(Box::new(self.parse_unary()));
        }
        if self.match_token(&DataviewToken::LParen) {
            let expression = self.parse_or();
            self.match_token(&DataviewToken::RParen);
            return expression;
        }
        self.parse_predicate()
    }

    fn parse_predicate(&mut self) -> DataviewExpression {
        let Some(first) = self.consume_value() else {
            return DataviewExpression::Predicate {
                field: None,
                operator: DataviewOperator::Contains,
                value: String::new(),
            };
        };

        if self.match_token(&DataviewToken::Colon) {
            return DataviewExpression::Predicate {
                field: Some(first),
                operator: DataviewOperator::Contains,
                value: self.consume_value().unwrap_or_default(),
            };
        }
        if self.match_token(&DataviewToken::Equals) {
            return DataviewExpression::Predicate {
                field: Some(first),
                operator: DataviewOperator::Equals,
                value: self.consume_value().unwrap_or_default(),
            };
        }
        if self.match_token(&DataviewToken::Tilde) {
            return DataviewExpression::Predicate {
                field: Some(first),
                operator: DataviewOperator::Fuzzy,
                value: self.consume_value().unwrap_or_default(),
            };
        }
        if self.match_word("is") {
            let is_not = self.match_word("not");
            return DataviewExpression::Predicate {
                field: Some(first),
                operator: if is_not {
                    DataviewOperator::IsNot
                } else {
                    DataviewOperator::Is
                },
                value: self.consume_value().unwrap_or_default(),
            };
        }

        DataviewExpression::Predicate {
            field: None,
            operator: DataviewOperator::Contains,
            value: first,
        }
    }

    fn consume_value(&mut self) -> Option<String> {
        let token = self.tokens.get(self.index)?;
        let value = match token {
            DataviewToken::Word(value) | DataviewToken::Quoted(value) => value.clone(),
            _ => return None,
        };
        self.index += 1;
        Some(value)
    }

    fn has_more(&self) -> bool {
        self.index < self.tokens.len()
    }

    fn peek_rparen(&self) -> bool {
        matches!(self.tokens.get(self.index), Some(DataviewToken::RParen))
    }

    fn peek_word(&self, value: &str) -> bool {
        matches!(self.tokens.get(self.index), Some(DataviewToken::Word(word)) if word.eq_ignore_ascii_case(value))
    }

    fn match_word(&mut self, value: &str) -> bool {
        if !self.peek_word(value) {
            return false;
        }
        self.index += 1;
        true
    }

    fn match_token(&mut self, token: &DataviewToken) -> bool {
        if self.tokens.get(self.index) != Some(token) {
            return false;
        }
        self.index += 1;
        true
    }
}

fn evaluate_dataview_expression(
    document: &DataviewDocument,
    expression: &DataviewExpression,
) -> bool {
    match expression {
        DataviewExpression::And(left, right) => {
            evaluate_dataview_expression(document, left)
                && evaluate_dataview_expression(document, right)
        }
        DataviewExpression::Or(left, right) => {
            evaluate_dataview_expression(document, left)
                || evaluate_dataview_expression(document, right)
        }
        DataviewExpression::Not(inner) => !evaluate_dataview_expression(document, inner),
        DataviewExpression::Predicate {
            field,
            operator,
            value,
        } => evaluate_dataview_predicate(document, field.as_deref(), operator, value),
    }
}

fn evaluate_dataview_predicate(
    document: &DataviewDocument,
    field: Option<&str>,
    operator: &DataviewOperator,
    value: &str,
) -> bool {
    if field.is_some_and(|field| {
        ["page.maintenance_user", "page.maintenance_group"]
            .iter()
            .any(|key| field.trim().eq_ignore_ascii_case(key))
    }) {
        let values = dataview_field_values(document, field);
        return values.iter().any(|candidate| match operator {
            DataviewOperator::IsNot => candidate != value,
            DataviewOperator::Equals | DataviewOperator::Is => candidate == value,
            DataviewOperator::Contains => wildcard_or_contains(candidate, value),
            DataviewOperator::Fuzzy => fuzzy_subsequence(candidate, value),
        });
    }
    let values = dataview_field_values(document, field);
    if values.is_empty() {
        return false;
    }
    match operator {
        DataviewOperator::IsNot => !values
            .iter()
            .any(|candidate| dataview_value_matches(candidate, &DataviewOperator::Is, value)),
        _ => values
            .iter()
            .any(|candidate| dataview_value_matches(candidate, operator, value)),
    }
}

fn dataview_field_values(document: &DataviewDocument, field: Option<&str>) -> Vec<String> {
    let Some(field) = field else {
        return vec![format!("{}\n{}", document.title, document.content)];
    };
    if field.trim().eq_ignore_ascii_case("page.identifier") {
        return vec![document.human_identifier.clone().unwrap_or_default()];
    }
    if field.trim().eq_ignore_ascii_case("page.maintenance_user") {
        return vec![document.maintenance_user_id.clone().unwrap_or_default()];
    }
    if field.trim().eq_ignore_ascii_case("page.maintenance_group") {
        return vec![document.maintenance_group_id.clone().unwrap_or_default()];
    }
    let normalized = normalize_dataview_key(field);
    match normalized.as_str() {
        "title" => vec![document.title.clone()],
        "content" | "body" => vec![document.content.clone()],
        "space" | "spacename" => vec![document.space_name.clone(), document.space_id.clone()],
        "spaceid" => vec![document.space_id.clone()],
        "tag" | "tags" => document.tags.clone(),
        "folder" | "foldername" => [document.folder_name.clone(), document.folder_id.clone()]
            .into_iter()
            .flatten()
            .collect(),
        "folderid" => document.folder_id.clone().into_iter().collect(),
        "user" | "author" | "owner" | "username" => {
            vec![document.author.clone(), document.author_id.clone()]
        }
        "userid" | "authorid" | "ownerid" => vec![document.author_id.clone()],
        "created" | "createdat" => vec![document.created_at.clone()],
        "updated" | "updatedat" => vec![document.updated_at.clone()],
        _ => document
            .properties
            .iter()
            .filter(|(name, _)| normalize_dataview_key(name) == normalized)
            .map(|(_, value)| value.clone())
            .collect(),
    }
}

fn dataview_value_matches(candidate: &str, operator: &DataviewOperator, value: &str) -> bool {
    let candidate = normalize_dataview_value(candidate);
    let value = normalize_dataview_value(value.trim_start_matches('#'));
    match operator {
        DataviewOperator::Contains => wildcard_or_contains(&candidate, &value),
        DataviewOperator::Equals | DataviewOperator::Is => {
            normalize_dataview_date(&candidate) == normalize_dataview_date(&value)
        }
        DataviewOperator::Fuzzy => fuzzy_subsequence(&candidate, &value),
        DataviewOperator::IsNot => unreachable!("is_not is handled by evaluate_dataview_predicate"),
    }
}

fn wildcard_or_contains(candidate: &str, value: &str) -> bool {
    if value == "*" {
        return true;
    }
    if value.contains('*') {
        let starts_with_wildcard = value.starts_with('*');
        let ends_with_wildcard = value.ends_with('*');
        let needle = value.trim_matches('*');
        return match (starts_with_wildcard, ends_with_wildcard) {
            (true, true) => candidate.contains(needle),
            (true, false) => candidate.ends_with(needle),
            (false, true) => candidate.starts_with(needle),
            (false, false) => candidate == value,
        };
    }
    candidate.contains(value)
}

fn fuzzy_subsequence(candidate: &str, value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let mut chars = value.chars();
    let mut current = chars.next();
    for candidate_char in candidate.chars() {
        if Some(candidate_char) == current {
            current = chars.next();
            if current.is_none() {
                return true;
            }
        }
    }
    false
}

fn normalize_dataview_key(value: &str) -> String {
    value
        .trim_start_matches('#')
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

fn normalize_dataview_value(value: &str) -> String {
    value.trim().trim_matches('"').to_lowercase()
}

fn normalize_dataview_date(value: &str) -> String {
    let date = value.split('T').next().unwrap_or(value);
    let parts = date.split('-').collect::<Vec<_>>();
    if parts.len() == 3 {
        if let (Ok(year), Ok(month), Ok(day)) = (
            parts[0].parse::<i32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<u32>(),
        ) {
            return format!("{:04}-{:02}-{:02}", year, month, day);
        }
    }
    value.to_string()
}

fn is_valid_search_date(value: Option<&str>) -> bool {
    value.is_some_and(|date| {
        date.len() == 10
            && date.chars().enumerate().all(|(index, ch)| {
                matches!(index, 4 | 7) == (ch == '-') && (ch.is_ascii_digit() || ch == '-')
            })
    })
}

/// POST /api/v1/admin/search/reindex — rebuild the entire search index
async fn reindex(
    State(state): State<AppState>,
    admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    crate::auth::middleware::require_current_admin(&mut conn, &admin).await?;
    // Collect complete source data before changing the existing index.
    let docs: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT id, space_id, title, content_path \
             FROM documents WHERE published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    // Fetch tags for each doc
    let mut doc_entries = Vec::new();
    for (id, space_id, title, content_path) in docs {
        let body = crate::storage::read_document(&content_path)
            .await
            .map_err(AppError::internal)?;

        let tags: Vec<(String,)> = sqlx::query_as(
            "SELECT t.name FROM tags t JOIN doc_tags dt ON t.id = dt.tag_id WHERE dt.document_id = $1 AND t.space_id = $2 ORDER BY t.id",
        )
        .bind(&id)
        .bind(&space_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?;

        let tag_str = tags.into_iter().map(|t| t.0).collect::<Vec<_>>().join(" ");

        doc_entries.push(index::DocEntry {
            doc_id: id,
            space_id,
            title,
            body,
            tags: tag_str,
        });
    }

    let count = doc_entries.len();
    crate::auth::middleware::require_current_admin(&mut conn, &admin).await?;

    state
        .search_index
        .rebuild(doc_entries)
        .map_err(|e| AppError::Internal(format!("Reindex failed: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "status": "reindex_complete", "documents_indexed": count })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::any::AnyPoolOptions;

    async fn reindex_request(state: AppState) -> axum::response::Response {
        use tower::ServiceExt;
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('rebuild-key','owner-a','Rebuild',$1,'test',0,'[\"admin\"]')")
            .bind(crate::auth::api_keys::hash_api_key("rebuild-fixture-key")).execute(&state.db).await.unwrap();
        router().with_state(state).oneshot(axum::http::Request::builder()
            .method("POST").uri("/admin/search/reindex").header("X-API-Key", "rebuild-fixture-key")
            .body(axum::body::Body::empty()).unwrap()).await.unwrap()
    }
    fn seed_reindex_old(state: &AppState) {
        state.search_index.rebuild(vec![index::DocEntry {
            doc_id: "retained".into(), space_id: "space-a".into(), title: "Retained".into(),
            body: "retainedtoken".into(), tags: String::new(),
        }]).unwrap();
    }
    #[tokio::test]
    async fn reindex_file_failure_returns_error_without_replacing_previous_index() {
        let state = actual_state().await;
        seed_reindex_old(&state);
        let path = body(&state, "target-a", "will be unreadable").await;
        tokio::fs::write(&path, [0xff, 0xfe]).await.unwrap();
        let response = reindex_request(state.clone()).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(state.search_index.search("retainedtoken", None, 10).unwrap()[0].doc_id, "retained");
        tokio::fs::remove_file(path).await.unwrap();
    }
    #[tokio::test]
    async fn reindex_tag_query_failure_returns_error_without_replacing_previous_index() {
        let state = actual_state().await;
        seed_reindex_old(&state);
        let a = body(&state, "target-a", "nebula").await;
        let b = body(&state, "target-b", "comet").await;
        sqlx::query("DROP TABLE doc_tags").execute(&state.db).await.unwrap();
        let response = reindex_request(state.clone()).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(state.search_index.search("retainedtoken", None, 10).unwrap()[0].doc_id, "retained");
        tokio::fs::remove_file(a).await.unwrap(); tokio::fs::remove_file(b).await.unwrap();
    }
    #[tokio::test]
    async fn reindex_success_has_current_bodies_scoped_tags_and_no_unpublished_page() {
        let state = actual_state().await;
        let a = body(&state, "target-a", "recovered nebula").await;
        let b = body(&state, "target-b", "privatedrafttoken").await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-b'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO tags(id,space_id,name) VALUES('own-tag','space-a','scopedtag'),('foreign-tag','space-b','foreigntag')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES('target-a','own-tag'),('target-a','foreign-tag')").execute(&state.db).await.unwrap();
        let response = reindex_request(state.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(state.search_index.search("nebula", None, 10).unwrap()[0].doc_id, "target-a");
        assert_eq!(state.search_index.search("scopedtag", None, 10).unwrap().len(), 1);
        assert!(state.search_index.search("foreigntag", None, 10).unwrap().is_empty());
        assert!(state.search_index.search("privatedrafttoken", None, 10).unwrap().is_empty());
        let visible = search_visible_pages(&state, &auth("viewer-a"), &query("nebula", 10)).await.unwrap();
        assert_eq!(visible.len(), 1); assert_eq!(visible[0].document_id, "target-a");
        tokio::fs::remove_file(a).await.unwrap(); tokio::fs::remove_file(b).await.unwrap();
    }

    #[tokio::test]
    async fn attachment_filter_requires_current_authorized_origin_for_positive_and_negative_queries() {
        let state=actual_state().await;
        let id="11111111-1111-4111-8111-111111111111";
        sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES($1,'target-a','space-a','owned.bin','Owned.bin','application/octet-stream',1,'spaces/space-a/attachments/target-a/owned.bin','owner-a')")
            .bind(id).execute(&state.db).await.unwrap();
        let mut conn=state.db.acquire().await.unwrap();
        let row=published_row(&mut conn,"target-a").await.unwrap().unwrap();
        let reader=auth("viewer-a");
        let mut query=SearchQuery {has_attachment:Some(true),..Default::default()};
        assert!(!current_filters(&mut conn,&reader,&row,&query).await.unwrap(),"Unclassified attachment rows are not a readable-asset signal");
        query.has_attachment=Some(false);
        assert!(current_filters(&mut conn,&reader,&row,&query).await.unwrap(),"Negative predicates must also use the authorized audience");
        sqlx::query("INSERT INTO attachment_audience_origins(asset_id,kind,original_space_id,original_document_id) VALUES($1,'legacy_page_v1','space-a','target-a')")
            .bind(id).execute(&mut *conn).await.unwrap();
        query.has_attachment=Some(true);
        assert!(current_filters(&mut conn,&reader,&row,&query).await.unwrap(),"Explicit legacy page-owned records retain their existing viewer policy");
        query.has_attachment=Some(false);
        assert!(!current_filters(&mut conn,&reader,&row,&query).await.unwrap());
        sqlx::query("DELETE FROM attachment_audience_origins WHERE asset_id=$1").bind(id).execute(&mut *conn).await.unwrap();
        assert!(current_filters(&mut conn,&reader,&row,&query).await.unwrap(),"A missing binding must never inherit legacy authority from current attachment existence");
    }

    #[tokio::test]
    async fn search_title_filter_matches_document_title_metadata() {
        let state = actual_state().await;
        sqlx::query("UPDATE documents SET title='Launch Plan' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let row = published_row(&mut conn, "target-a").await.unwrap().unwrap();
        let mut q = SearchQuery {
            title: Some("launch".into()),
            ..Default::default()
        };
        assert!(current_filters(&mut conn, &auth("viewer-a"), &row, &q).await.unwrap());
        q.title = Some("retrospective".into());
        assert!(!current_filters(&mut conn, &auth("viewer-a"), &row, &q).await.unwrap());
    }
    #[tokio::test]
    async fn search_user_filter_matches_id_name_or_email() {
        let state = actual_state().await;
        sqlx::query("UPDATE users SET display_name='Harley Frank',email='harley@example.com' WHERE id='owner-a'").execute(&state.db).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let row = published_row(&mut conn, "target-a").await.unwrap().unwrap();
        for value in ["owner-a", "Harley Frank", "harley@example.com"] {
            let q = SearchQuery {
                user: Some(value.into()),
                ..Default::default()
            };
            assert!(current_filters(&mut conn, &auth("viewer-a"), &row, &q).await.unwrap());
        }
    }

    fn doc() -> DataviewDocument {
        DataviewDocument {
            id: "doc-a".to_string(),
            title: "Project Alpha Plan".to_string(),
            human_identifier: None,
            maintenance_user_id: None,
            maintenance_group_id: None,
            space_id: "field-notes".to_string(),
            space_name: "Field Notes".to_string(),
            content: "Launch checklist and owner notes".to_string(),
            tags: vec!["project-alpha".to_string(), "planning".to_string()],
            properties: vec![
                ("Status".to_string(), "Draft".to_string()),
                ("ownerName".to_string(), "Eagle".to_string()),
                ("reviewDate".to_string(), "2026-7-6".to_string()),
            ],
            created_at: "2026-07-01T00:00:00Z".to_string(),
            updated_at: "2026-07-06T00:00:00Z".to_string(),
            author: "Church".to_string(),
            author_id: "user-church".to_string(),
            folder_id: Some("folder-ops".to_string()),
            folder_name: Some("Operations".to_string()),
        }
    }

    #[test]
    fn maintenance_filters_preserve_custom_owner_and_author_meaning() {
        let mut page = doc();
        page.maintenance_group_id = Some("Engineering-ID".into());
        page.properties
            .push(("Owner".into(), "Legacy Owner".into()));
        for (query, expected) in [
            ("page.maintenance_group = Engineering-ID", true),
            ("page.maintenance_group = another-team", false),
            ("page.maintenance_group = engineering-id", false),
            ("page.maintenance_user is \"\"", true),
            ("Owner = Church", true),
            (
                "Status = Draft AND page.maintenance_group = Engineering-ID",
                true,
            ),
        ] {
            assert_eq!(
                evaluate_dataview_expression(&page, &parse_dataview_query(query).unwrap()),
                expected,
                "{query}"
            );
        }
        let q = SearchQuery {
            maintenance_group: Some("Engineering-ID".into()),
            ..Default::default()
        };
        assert!(maintenance_matches(None, Some("Engineering-ID"), &q));
        assert!(!maintenance_matches(None, Some("engineering-id"), &q));
        assert!(!maintenance_matches(Some("Engineering-ID"), None, &q));
    }

    #[test]
    fn readable_identifier_does_not_shadow_custom_identifier_or_owner_fields() {
        let mut page = doc();
        page.human_identifier = Some("Ops-17".into());
        page.properties.extend([
            ("Identifier".into(), "legacy".into()),
            ("PageIdentifier".into(), "custom".into()),
        ]);
        for (query, expected) in [
            ("page.identifier = ops-17", true),
            ("Identifier = ops-17", false),
            ("Identifier = legacy", true),
            ("PageIdentifier = custom", true),
            ("property.Identifier = legacy", false),
            ("owner = Church", true),
        ] {
            assert_eq!(
                evaluate_dataview_expression(&page, &parse_dataview_query(query).unwrap()),
                expected,
                "{query}"
            );
        }
        assert!(identifier_matches(Some("Ops-17"), Some(" ops-17 ")));
        assert!(!identifier_matches(None, Some("ops-17")));
        assert!(!identifier_matches(Some("Ops-17"), Some("Ops-1*")));
    }

    #[test]
    fn dataview_parser_matches_tags_properties_and_dates() {
        let expression = parse_dataview_query(
            "title:\"Project Alpha\" AND tag:#planning AND ownerName:Eagle AND reviewDate is 2026-07-06",
        )
        .expect("dataview expression");

        assert!(evaluate_dataview_expression(&doc(), &expression));
    }

    #[test]
    fn dataview_parser_supports_boolean_and_fuzzy_predicates() {
        let matching = parse_dataview_query("(space:field-notes OR tag:ops) AND title~PjAha")
            .expect("matching expression");
        let blocked =
            parse_dataview_query("tag:planning AND NOT Status:Draft").expect("blocked expression");

        assert!(evaluate_dataview_expression(&doc(), &matching));
        assert!(!evaluate_dataview_expression(&doc(), &blocked));
    }

    #[test]
    fn dataview_parser_matches_folder_and_user_multi_value_groups() {
        let expression = parse_dataview_query(
            "(folder:Operations OR folder:Archive) AND (user:Church OR user:Tucker)",
        )
        .expect("folder and user expression");

        assert!(evaluate_dataview_expression(&doc(), &expression));
    }

    #[test]
    fn dataview_result_limit_defaults_to_documented_api_limit_and_caps_large_requests() {
        assert_eq!(dataview_result_limit(None), 50);
        assert_eq!(dataview_result_limit(Some(0)), 0);
        assert_eq!(dataview_result_limit(Some(12)), 12);
        assert_eq!(dataview_result_limit(Some(250)), 100);
    }

    thread_local! {static SEARCH_READ_PAUSE:std::cell::RefCell<Option<(tokio::sync::oneshot::Sender<()>,tokio::sync::oneshot::Receiver<()>)>>=const{std::cell::RefCell::new(None)};}
    thread_local! {static SEARCH_FINAL_PAUSE:std::cell::RefCell<Option<(tokio::sync::oneshot::Sender<()>,tokio::sync::oneshot::Receiver<()>)>>=const{std::cell::RefCell::new(None)};}
    pub(super) async fn pause_search_final() {
        let pause = SEARCH_FINAL_PAUSE.with(|s| s.borrow_mut().take());
        if let Some((ready, resume)) = pause {
            let _ = ready.send(());
            resume.await.unwrap();
        }
    }
    pub(super) async fn pause_search_read() {
        let pause = SEARCH_READ_PAUSE.with(|s| s.borrow_mut().take());
        if let Some((ready, resume)) = pause {
            let _ = ready.send(());
            resume.await.unwrap();
        }
    }
    fn auth(actor: &str) -> AuthUser {
        let mut auth = crate::spaces::properties::tests::test_auth(actor);
        auth.claims.sid = format!("search-session-{actor}");
        auth
    }
    async fn actual_state() -> AppState {
        let state = crate::spaces::properties::tests::property_test_state().await;
        for actor in ["owner-a", "editor-a", "viewer-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'search','2999-01-01T00:00:00Z')").bind(format!("search-session-{actor}")).bind(actor).execute(&state.db).await.unwrap();
        }
        state
    }
    async fn read_key(state: &AppState) -> AuthUser {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('search-key','owner-a','Search','hash','test',0,'[\"write\"]')").execute(&state.db).await.unwrap();
        let mut auth = auth("owner-a");
        auth.claims.sid = "api-key:search-key".into();
        auth.api_key_scopes = Some(vec!["admin".into()]);
        auth
    }
    async fn body(state: &AppState, id: &str, text: &str) -> String {
        let path = std::env::temp_dir().join(format!("search-body-{}.md", uuid::Uuid::new_v4()));
        tokio::fs::write(&path, text).await.unwrap();
        let path = path.to_string_lossy().to_string();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id=$2")
            .bind(&path)
            .bind(id)
            .execute(&state.db)
            .await
            .unwrap();
        path
    }
    fn query(q: &str, limit: usize) -> SearchQuery {
        SearchQuery {
            q: Some(q.into()),
            limit: Some(limit),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn current_search_omits_stale_private_index_sources_and_uses_actual_space_labels() {
        let state = actual_state().await;
        let path = body(&state, "target-a", "Current public nebula").await;
        state
            .search_index
            .rebuild(vec![index::DocEntry {
                doc_id: "target-a".into(),
                space_id: "space-b".into(),
                title: "Old private title".into(),
                body: "Secret nebula".into(),
                tags: String::new(),
            }])
            .unwrap();
        assert!(
            search_visible_pages(&state, &auth("owner-a"), &query("nebula", 20))
                .await
                .unwrap()
                .is_empty()
        );
        state
            .search_index
            .upsert(index::DocEntry {
                doc_id: "target-a".into(),
                space_id: "space-b".into(),
                title: "target-a".into(),
                body: "Current public nebula".into(),
                tags: String::new(),
            })
            .unwrap();
        assert!(
            search_visible_pages(&state, &auth("owner-a"), &query("space_id:space-b", 20))
                .await
                .unwrap()
                .is_empty()
        );
        state
            .search_index
            .upsert(index::DocEntry {
                doc_id: "target-a".into(),
                space_id: "space-a".into(),
                title: "target-a".into(),
                body: "Current public nebula".into(),
                tags: String::new(),
            })
            .unwrap();
        let mut q = query("nebula", 20);
        q.space_id = Some("space-a".into());
        let results = search_visible_pages(&state, &auth("owner-a"), &q)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].space_id, "space-a");
        assert_eq!(results[0].title, "target-a");
        assert!(!results[0].excerpt.contains("Secret"));
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(search_visible_pages(&state, &auth("owner-a"), &q)
            .await
            .unwrap()
            .is_empty());
        tokio::fs::remove_file(path).await.unwrap();
    }
    #[tokio::test]
    async fn stale_duplicate_hit_does_not_consume_current_identity() {
        let state = actual_state().await;
        let path = body(&state, "target-a", "nebula current").await;
        state
            .search_index
            .rebuild(vec![
                index::DocEntry {
                    doc_id: "target-a".into(),
                    space_id: "space-a".into(),
                    title: "Old".into(),
                    body: "nebula nebula nebula nebula nebula".into(),
                    tags: String::new(),
                },
                index::DocEntry {
                    doc_id: "target-a".into(),
                    space_id: "space-a".into(),
                    title: "target-a".into(),
                    body: "nebula current".into(),
                    tags: String::new(),
                },
            ])
            .unwrap();
        let result = search_visible_pages(&state, &auth("owner-a"), &query("nebula", 1))
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "target-a");
        tokio::fs::remove_file(path).await.unwrap();
    }
    #[tokio::test]
    async fn indexed_and_sql_search_fill_visible_limit_past_hidden_pages() {
        let state = actual_state().await;
        let path = body(&state, "target-a", "Signal nebula").await;
        let mut entries = Vec::new();
        for n in 0..530 {
            let id = format!("hidden-{n:03}");
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,updated_at) VALUES($1,'space-b',$1,'','owner-a','owner-a','old','2999')").bind(&id).execute(&state.db).await.unwrap();
            entries.push(index::DocEntry {
                doc_id: id,
                space_id: "space-b".into(),
                title: "Signal".into(),
                body: "nebula nebula nebula nebula".into(),
                tags: String::new(),
            });
        }
        entries.push(index::DocEntry {
            doc_id: "target-a".into(),
            space_id: "space-a".into(),
            title: "target-a".into(),
            body: "Signal nebula".into(),
            tags: String::new(),
        });
        state.search_index.rebuild(entries).unwrap();
        let indexed = search_visible_pages(&state, &auth("owner-a"), &query("nebula", 1))
            .await
            .unwrap();
        assert_eq!(indexed.len(), 1);
        assert_eq!(indexed[0].document_id, "target-a");
        let sql = search_visible_pages(&state, &auth("owner-a"), &query("", 1))
            .await
            .unwrap();
        assert_eq!(sql.len(), 1);
        assert_eq!(sql[0].document_id, "target-a");
        let response = dataview(
            State(state.clone()),
            auth("owner-a"),
            Query(DataviewQueryParams {
                q: Some("content:nebula".into()),
                limit: Some(1),
            }),
        )
        .await
        .unwrap()
        .into_response();
        let bytes = axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json[0]["document_id"], "target-a");
        tokio::fs::remove_file(path).await.unwrap();
    }
    #[tokio::test]
    async fn search_current_human_read_proof_applies_to_empty_queries_and_keys() {
        let state = actual_state().await;
        let key = read_key(&state).await;
        assert!(search_visible_pages(&state, &key, &query("", 0))
            .await
            .is_err());
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(search_visible_pages(&state, &key, &query("", 0))
            .await
            .is_ok());
        let mut service = AuthUser {
            claims: key.claims.clone(),
            api_key_scopes: key.api_key_scopes.clone(),
        };
        service.claims.sub = "service:search-key".into();
        service.claims.sid = "service:search-key".into();
        assert!(search_visible_pages(&state, &service, &query("", 0))
            .await
            .is_err());
        sqlx::query("DELETE FROM sessions WHERE id='search-session-owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            search_visible_pages(&state, &auth("owner-a"), &query("", 0))
                .await
                .is_err()
        );
        sqlx::query("UPDATE users SET is_active=0 WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(search_visible_pages(&state, &key, &query("", 0))
            .await
            .is_err());
    }
    #[tokio::test]
    async fn draft_search_requires_exact_recognized_share_and_active_private_ancestors() {
        let state = actual_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('private','target-a','space-a','editor-a','Private','---\n malformed: [\n---\n nebula raw body','base')").execute(&state.db).await.unwrap();
        let mut q = query("nebula", 20);
        q.is_draft = Some(true);
        assert!(search_visible_pages(&state, &auth("owner-a"), &q)
            .await
            .unwrap()
            .is_empty());
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('private','owner-a','viewer','editor-a')").execute(&state.db).await.unwrap();
        let results = search_visible_pages(&state, &auth("owner-a"), &q)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].draft_id.as_deref(), Some("private"));
        assert!(results[0].excerpt.contains("malformed"));
        sqlx::query("PRAGMA ignore_check_constraints=ON")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE document_draft_access SET role='invented'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(search_visible_pages(&state, &auth("owner-a"), &q)
            .await
            .unwrap()
            .is_empty());
        sqlx::query("UPDATE document_draft_access SET role='viewer'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("PRAGMA ignore_check_constraints=OFF")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('parent','space-a','Hidden parent','','editor-a','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='parent' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(search_visible_pages(&state, &auth("owner-a"), &q)
            .await
            .unwrap()
            .is_empty());
    }
    #[tokio::test]
    async fn draft_search_fills_visible_limit_after_inaccessible_ancestor_candidates() {
        let state = actual_state().await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('private-parent','space-a','Parent','','editor-a','editor-a'),('visible-draft-page','space-a','Visible','','owner-a','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('visible-draft','visible-draft-page','space-a','owner-a','Visible','nebula readable','base','2000')").execute(&state.db).await.unwrap();
        for n in 0..120 {
            let id = format!("hidden-draft-{n}");
            sqlx::query("INSERT INTO documents(id,space_id,parent_id,title,content_path,created_by,last_updated_by) VALUES($1,'space-a','private-parent',$1,'','owner-a','owner-a')").bind(&id).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES($1,$1,'space-a','owner-a',$1,'nebula hidden ancestor','base','2999')").bind(&id).execute(&state.db).await.unwrap();
        }
        let mut q = query("nebula", 1);
        q.is_draft = Some(true);
        let result = search_visible_pages(&state, &auth("owner-a"), &q)
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].draft_id.as_deref(), Some("visible-draft"));
    }

    #[test]
    fn index_source_proof_accepts_published_portable_composition_and_unicode_excerpt() {
        let mut chars = "界".repeat(240);
        chars.push_str(" nebula");
        let excerpt = index::excerpt(&chars, 200);
        assert_eq!(excerpt.chars().count(), 203);
        let frontmatter =
            crate::spaces::documents::document_storage_frontmatter("Title", None, false, false);
        let portable = "---\npriority: high\n---\n Body";
        let source = CurrentPublished {
            grant_only:false,grant:None,allow_grants:false,body_loaded:true,proofs:Vec::new(),
            row: PublishedRow {
                id: "id".into(),
                title: "Title".into(),
                human_identifier: None,
                maintenance_user_id: None,
                maintenance_group_id: None,
                space_id: "space".into(),
                space_name: "Space".into(),
                author: "Author".into(),
                author_id: "actor".into(),
                author_email: String::new(),
                updated_at: "v1".into(),
                created_at: "v1".into(),
                content_path: "path".into(),
                published_at: "v1".into(),
                folder_id: None,
                folder_name: None,
                icon: None,
                locked: 0,
                is_public: 0,
            },
            content: format!("---\n{frontmatter}\npriority: high\n---\n\nBody"),
            tags: Vec::new(),
            properties: Vec::new(),
        };
        assert!(indexed_source_matches(
            &index::SearchCandidate {
                doc_id: "id".into(),
                space_id: "space".into(),
                title: "Title".into(),
                body: portable.into(),
                tags: String::new()
            },
            &source
        ));
        assert!(!indexed_source_matches(
            &index::SearchCandidate {
                doc_id: "id".into(),
                space_id: "space".into(),
                title: "Title".into(),
                body: "---\nsecret: old\n---\n Body".into(),
                tags: String::new()
            },
            &source
        ));
    }
    #[tokio::test]
    async fn search_snapshot_paging_deduplicates_without_losing_visible_rows() {
        let state = actual_state().await;
        let path = body(&state, "target-a", "nebula").await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('second','space-a','Second',$1,'owner-a','owner-a','old')").bind(&path).execute(&state.db).await.unwrap();
        let entries = (0..150)
            .map(|_| index::DocEntry {
                doc_id: "target-a".into(),
                space_id: "space-a".into(),
                title: "target-a".into(),
                body: "nebula".into(),
                tags: String::new(),
            })
            .chain(std::iter::once(index::DocEntry {
                doc_id: "second".into(),
                space_id: "space-a".into(),
                title: "Second".into(),
                body: "nebula".into(),
                tags: String::new(),
            }))
            .collect();
        state.search_index.rebuild(entries).unwrap();
        let snapshot = state.search_index.snapshot("nebula").unwrap();
        let first = snapshot.page(0, 100).unwrap();
        state
            .search_index
            .rebuild(vec![index::DocEntry {
                doc_id: "new-index-only".into(),
                space_id: "space-a".into(),
                title: "New".into(),
                body: "nebula".into(),
                tags: String::new(),
            }])
            .unwrap();
        let second = snapshot.page(100, 100).unwrap();
        assert_eq!(first.len() + second.len(), 151);
        assert!(second.iter().all(|hit| hit.doc_id != "new-index-only"));
        // Restore the duplicate-bearing index to verify route-wide dedupe and
        // continuing past a complete duplicate raw page to the second identity.
        let entries = first
            .into_iter()
            .chain(second)
            .map(|hit| index::DocEntry {
                doc_id: hit.doc_id,
                space_id: "space-a".into(),
                title: hit.title,
                body: hit.body,
                tags: hit.tags,
            })
            .collect();
        state.search_index.rebuild(entries).unwrap();
        let results = search_visible_pages(&state, &auth("owner-a"), &query("nebula", 2))
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_ne!(results[0].document_id, results[1].document_id);
        tokio::fs::remove_file(path).await.unwrap();
    }
    #[tokio::test]
    async fn search_read_rechecks_external_wal_credential_source_and_draft_body_changes() {
        for variant in [
            "credential",
            "published-source",
            "draft-body",
            "dataview-source",
            "final-block",
            "final-unpublish",
            "final-draft-share",
            "final-dataview-block",
            "final-identifier",
            "final-dataview-identifier",
            "final-draft-identifier",
            "final-maintenance",
            "final-dataview-maintenance",
            "final-draft-maintenance",
        ] {
            let mut state = actual_state().await;
            let dbfile =
                std::env::temp_dir().join(format!("search-wal-{}.sqlite", uuid::Uuid::new_v4()));
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
                .connect(&format!("sqlite://{}?mode=rwc", dbfile.display()))
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            for sql in ["INSERT INTO users(id,email,display_name,role) VALUES('owner-a','wal@example.test','Owner','member')","INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('search-session-owner-a','owner-a','hash','2999-01-01T00:00:00Z')","INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','A','a','owner-a')","INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner','space-a','owner-a','owner')","INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','target-a','','owner-a','owner-a','old')"]{sqlx::query(sql).execute(&state.db).await.unwrap();}
            let path = body(&state, "target-a", "nebula current body").await;
            if variant == "draft-body"
                || variant == "final-draft-identifier"
                || variant == "final-draft-maintenance"
            {
                sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
                    .execute(&state.db)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('draft','target-a','space-a','owner-a','Draft','nebula private draft','base')").execute(&state.db).await.unwrap();
            }
            if variant == "final-unpublish" {
                sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('other-draft','target-a','space-a','owner-a','Other','other readable participation','base')").execute(&state.db).await.unwrap();
            }
            if variant == "final-draft-share" {
                for sql in [
                    "INSERT INTO users(id,email,display_name,role) VALUES('editor-a','other@example.test','Editor','member')",
                    "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('search-session-editor-a','editor-a','hash','2999-01-01T00:00:00Z')",
                    "INSERT INTO space_members(id,space_id,user_id,role) VALUES('editor','space-a','editor-a','editor')",
                    "UPDATE documents SET published_at=NULL WHERE id='target-a'",
                    "INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('selected','target-a','space-a','owner-a','Selected','nebula selected secret','base')",
                    "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('selected','editor-a','viewer','owner-a')",
                    "INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('other-readable','target-a','space-a','editor-a','Other','different readable participation','base')",
                ] { sqlx::query(sql).execute(&state.db).await.unwrap(); }
            }
            if variant.contains("maintenance") {
                sqlx::query("INSERT INTO groups(id,name) VALUES('maintenance-team','Engineering')")
                    .execute(&state.db)
                    .await
                    .unwrap();
                sqlx::query("UPDATE documents SET maintenance_group_id='maintenance-team' WHERE id='target-a'").execute(&state.db).await.unwrap();
            }
            if variant.contains("identifier") {
                sqlx::query("UPDATE documents SET human_identifier='Ops-17',human_identifier_key='OPS-17' WHERE id='target-a'").execute(&state.db).await.unwrap();
            }
            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
            let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
            if variant.starts_with("final-") {
                SEARCH_FINAL_PAUSE.with(|slot| *slot.borrow_mut() = Some((ready_tx, resume_rx)));
            } else {
                SEARCH_READ_PAUSE.with(|slot| *slot.borrow_mut() = Some((ready_tx, resume_rx)));
            }
            let writer = state.db.clone();
            let change=match variant{"final-maintenance"|"final-dataview-maintenance"|"final-draft-maintenance"=>"DELETE FROM groups WHERE id='maintenance-team'","credential"=>"DELETE FROM sessions WHERE id='search-session-owner-a'","final-block"|"final-dataview-block"=>"INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')","final-unpublish"=>"UPDATE documents SET published_at=NULL WHERE id='target-a'","final-draft-share"=>"DELETE FROM document_draft_access WHERE draft_id='selected' AND user_id='editor-a'","final-identifier"|"final-dataview-identifier"|"final-draft-identifier"=>"UPDATE documents SET human_identifier='Ops-18',human_identifier_key='OPS-18' WHERE id='target-a'","draft-body"=>"UPDATE document_drafts SET content='replacement private body',updated_at='changed' WHERE id='draft'",_=>"UPDATE documents SET content_path='changed-unread-path',updated_at='changed' WHERE id='target-a'"};
            let task = tokio::spawn(async move {
                tokio::time::timeout(std::time::Duration::from_secs(5), ready_rx)
                    .await
                    .unwrap()
                    .unwrap();
                let maintenance_before: Option<String> = if variant.contains("maintenance") {
                    Some(
                        sqlx::query_scalar("SELECT updated_at FROM documents WHERE id='target-a'")
                            .fetch_one(&writer)
                            .await
                            .unwrap(),
                    )
                } else {
                    None
                };
                sqlx::query(change).execute(&writer).await.unwrap();
                if let Some(stamp) = maintenance_before {
                    let after: (String, Option<String>) = sqlx::query_as(
                        "SELECT updated_at,maintenance_group_id FROM documents WHERE id='target-a'",
                    )
                    .fetch_one(&writer)
                    .await
                    .unwrap();
                    assert_eq!(
                        after,
                        (stamp, None),
                        "actual FK clear must leave the stamp unchanged"
                    );
                }
                resume_tx.send(()).unwrap();
            });
            let response = if variant == "dataview-source"
                || variant == "final-dataview-block"
                || variant == "final-dataview-identifier"
                || variant == "final-dataview-maintenance"
            {
                dataview(
                    State(state.clone()),
                    auth("owner-a"),
                    Query(DataviewQueryParams {
                        q: Some(
                            if variant == "final-dataview-maintenance" {
                                "page.maintenance_group = maintenance-team"
                            } else if variant == "final-dataview-identifier" {
                                "page.identifier = ops-17"
                            } else {
                                "content:nebula"
                            }
                            .into(),
                        ),
                        limit: Some(20),
                    }),
                )
                .await
                .map(IntoResponse::into_response)
            } else {
                let mut q = query("", 20);
                if variant.contains("maintenance") {
                    q.maintenance_group = Some("maintenance-team".into());
                }
                if variant.contains("identifier") {
                    q.identifier = Some("ops-17".into());
                }
                if variant == "draft-body"
                    || variant == "final-draft-share"
                    || variant == "final-draft-identifier"
                    || variant == "final-draft-maintenance"
                {
                    q.q = Some("nebula".into());
                    q.is_draft = Some(true)
                }
                search(
                    State(state.clone()),
                    auth(if variant == "final-draft-share" {
                        "editor-a"
                    } else {
                        "owner-a"
                    }),
                    Query(q),
                )
                .await
                .map(IntoResponse::into_response)
            };
            task.await.unwrap();
            if variant == "credential" {
                assert!(response.is_err())
            } else {
                let bytes = axum::body::to_bytes(response.unwrap().into_body(), 10000)
                    .await
                    .unwrap();
                let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert!(json.as_array().unwrap().is_empty(), "{variant}");
            }
            if variant == "final-unpublish" || variant == "final-draft-share" {
                let actor = if variant == "final-draft-share" {
                    "editor-a"
                } else {
                    "owner-a"
                };
                let mut conn = state.db.acquire().await.unwrap();
                // Page participation remains available through another draft;
                // it cannot authorize the stale canonical or selected draft body.
                page_view(&mut conn, &auth(actor), "space-a", "target-a")
                    .await
                    .unwrap();
                if variant == "final-draft-share" {
                    crate::spaces::documents::require_discovery_draft_view(
                        &mut conn,
                        &auth(actor),
                        "space-a",
                        "target-a",
                        "other-readable",
                    )
                    .await
                    .unwrap();
                }
            }
            state.db.close().await;
            std::fs::remove_file(dbfile).unwrap();
            tokio::fs::remove_file(path).await.unwrap();
        }
    }
}

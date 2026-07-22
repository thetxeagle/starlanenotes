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

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub title: Option<String>,
    pub space_id: Option<String>,
    pub folder: Option<String>,
    pub tag: Option<String>,
    pub property_name: Option<String>,
    pub property_value: Option<String>,
    pub has_attachment: Option<bool>,
    pub has_cover: Option<bool>,
    pub has_workflow: Option<bool>,
    pub has_tag: Option<bool>,
    pub has_property: Option<bool>,
    pub is_locked: Option<bool>,
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
    pub title: String,
    pub space_id: String,
    pub space_name: String,
    pub excerpt: String,
    pub updated_at: String,
    pub author: String,
}

#[derive(Clone)]
struct DataviewDocument {
    id: String,
    title: String,
    space_id: String,
    space_name: String,
    content: String,
    tags: Vec<String>,
    properties: Vec<(String, String)>,
    created_at: String,
    updated_at: String,
    author: String,
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

/// GET /api/v1/search?q=term&title=optional&space_id=optional&folder=optional&tag=optional&property_name=optional&property_value=optional&limit=20
async fn search(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let q = query.q.unwrap_or_default().trim().to_string();
    let title = query
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let tag = query
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let folder = query
        .folder
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let property_name = query
        .property_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let property_value = query
        .property_value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let has_filters = title.is_some()
        || folder.is_some()
        || tag.is_some()
        || property_name.is_some()
        || property_value.is_some()
        || query.has_attachment.is_some()
        || query.has_cover.is_some()
        || query.has_workflow.is_some()
        || query.has_tag.is_some()
        || query.has_property.is_some()
        || query.is_locked.is_some()
        || is_valid_search_date(query.created_on.as_deref())
        || is_valid_search_date(query.updated_on.as_deref());

    if q.len() > 500 {
        return Err(AppError::BadRequest(
            "Search query must be at most 500 characters".into(),
        ));
    }

    let limit = query.limit.unwrap_or(20).min(100);
    let candidate_limit = if has_filters {
        (limit * 10).max(100)
    } else {
        limit
    };

    let mut results = Vec::new();

    if q.is_empty() {
        let docs: Vec<(String, String, String, String, String, String)> =
            if let Some(space_id) = query.space_id.as_deref() {
                sqlx::query_as(
                    "SELECT d.id, d.title, d.space_id, s.name, u.display_name, d.updated_at \
                 FROM documents d \
                 JOIN spaces s ON d.space_id = s.id \
                 JOIN users u ON d.last_updated_by = u.id \
                 WHERE d.space_id = ?1 \
                 ORDER BY d.updated_at DESC \
                 LIMIT ?2",
                )
                .bind(space_id)
                .bind(candidate_limit as i64)
                .fetch_all(&state.db)
                .await
                .map_err(AppError::internal)?
            } else {
                sqlx::query_as(
                    "SELECT d.id, d.title, d.space_id, s.name, u.display_name, d.updated_at \
                 FROM documents d \
                 JOIN spaces s ON d.space_id = s.id \
                 JOIN users u ON d.last_updated_by = u.id \
                 ORDER BY d.updated_at DESC \
                 LIMIT ?1",
                )
                .bind(candidate_limit as i64)
                .fetch_all(&state.db)
                .await
                .map_err(AppError::internal)?
            };

        for (document_id, document_title, space_id, space_name, author, updated_at) in docs {
            if !can_read_search_document(&state, &document_id, &auth.claims.sub).await? {
                continue;
            }
            if !matches_filters(
                &state,
                &document_id,
                title,
                folder,
                tag,
                property_name,
                property_value,
                query.has_attachment,
                query.has_cover,
                query.has_workflow,
                query.has_tag,
                query.has_property,
                query.is_locked,
                query.created_on.as_deref(),
                query.updated_on.as_deref(),
            )
            .await?
            {
                continue;
            }
            results.push(SearchResult {
                document_id,
                title: document_title,
                space_id,
                space_name,
                excerpt: String::new(),
                updated_at,
                author,
            });
            if results.len() >= limit {
                break;
            }
        }
    } else {
        // Search tantivy index, then apply structured filters from SQL metadata.
        let index_results = state
            .search_index
            .search(&q, query.space_id.as_deref(), candidate_limit)
            .map_err(|e| AppError::Internal(format!("Search error: {}", e)))?;

        for hit in index_results {
            if !can_read_search_document(&state, &hit.doc_id, &auth.claims.sub).await? {
                continue;
            }
            if !matches_filters(
                &state,
                &hit.doc_id,
                title,
                folder,
                tag,
                property_name,
                property_value,
                query.has_attachment,
                query.has_cover,
                query.has_workflow,
                query.has_tag,
                query.has_property,
                query.is_locked,
                query.created_on.as_deref(),
                query.updated_on.as_deref(),
            )
            .await?
            {
                continue;
            }

            let meta: Option<(String, String, String)> = sqlx::query_as(
                "SELECT s.name, u.display_name, d.updated_at \
                 FROM documents d \
                 JOIN spaces s ON d.space_id = s.id \
                 JOIN users u ON d.last_updated_by = u.id \
                 WHERE d.id = ?1",
            )
            .bind(&hit.doc_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

            if let Some((space_name, author, updated_at)) = meta {
                results.push(SearchResult {
                    document_id: hit.doc_id,
                    title: hit.title,
                    space_id: hit.space_id,
                    space_name,
                    excerpt: hit.excerpt,
                    updated_at,
                    author,
                });
            }

            if results.len() >= limit {
                break;
            }
        }
    }

    Ok(Json(results))
}

/// GET /api/v1/dataview?q=tag:roadmap&limit=50
async fn dataview(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<DataviewQueryParams>,
) -> Result<impl IntoResponse, AppError> {
    let q = query.q.unwrap_or_default().trim().to_string();
    if q.is_empty() {
        return Ok(Json(Vec::<SearchResult>::new()));
    }
    if q.len() > 500 {
        return Err(AppError::BadRequest(
            "Dataview query must be at most 500 characters".into(),
        ));
    }

    let limit = dataview_result_limit(query.limit);
    let expression = parse_dataview_query(&q);
    let documents = load_dataview_documents(&state, &auth, 500).await?;
    let results = documents
        .into_iter()
        .filter(|document| {
            expression
                .as_ref()
                .map(|expr| evaluate_dataview_expression(document, expr))
                .unwrap_or(true)
        })
        .take(limit)
        .map(|document| SearchResult {
            document_id: document.id,
            title: document.title,
            space_id: document.space_id,
            space_name: document.space_name,
            excerpt: document
                .content
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(180)
                .collect(),
            updated_at: document.updated_at,
            author: document.author,
        })
        .collect::<Vec<_>>();

    Ok(Json(results))
}

async fn load_dataview_documents(
    state: &AppState,
    auth: &AuthUser,
    candidate_limit: usize,
) -> Result<Vec<DataviewDocument>, AppError> {
    let rows: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = if auth.claims.role == "admin" {
        sqlx::query_as(
                "SELECT d.id, d.title, d.space_id, s.name, u.display_name, d.updated_at, d.created_at, d.content_path \
                 FROM documents d \
                 JOIN spaces s ON d.space_id = s.id \
                 JOIN users u ON d.last_updated_by = u.id \
                 ORDER BY d.updated_at DESC \
                 LIMIT ?1",
            )
            .bind(candidate_limit as i64)
            .fetch_all(&state.db)
            .await
            .map_err(AppError::internal)?
    } else {
        sqlx::query_as(
                "SELECT DISTINCT d.id, d.title, d.space_id, s.name, u.display_name, d.updated_at, d.created_at, d.content_path \
                 FROM documents d \
                 JOIN spaces s ON d.space_id = s.id \
                 JOIN users u ON d.last_updated_by = u.id \
                 LEFT JOIN space_members sm ON sm.space_id = d.space_id AND sm.user_id = ?1 \
                 LEFT JOIN space_group_permissions sgp ON sgp.space_id = d.space_id \
                 LEFT JOIN group_members gm ON gm.group_id = sgp.group_id AND gm.user_id = ?1 \
                 WHERE sm.user_id IS NOT NULL OR gm.user_id IS NOT NULL \
                 ORDER BY d.updated_at DESC \
                 LIMIT ?2",
            )
            .bind(&auth.claims.sub)
            .bind(candidate_limit as i64)
            .fetch_all(&state.db)
            .await
            .map_err(AppError::internal)?
    };

    let mut documents = Vec::new();
    for (id, title, space_id, space_name, author, updated_at, created_at, content_path) in rows {
        let content = crate::storage::read_document(&content_path)
            .await
            .unwrap_or_default();
        let tags: Vec<(String,)> = sqlx::query_as(
            "SELECT t.name FROM tags t \
             JOIN doc_tags dt ON t.id = dt.tag_id \
             WHERE dt.document_id = ?1",
        )
        .bind(&id)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;
        let properties: Vec<(String, String)> = sqlx::query_as(
            "SELECT ps.name, COALESCE(pv.value, '') \
             FROM property_values pv \
             JOIN properties_schema ps ON pv.property_id = ps.id \
             WHERE pv.document_id = ?1",
        )
        .bind(&id)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

        documents.push(DataviewDocument {
            id,
            title,
            space_id,
            space_name,
            content,
            tags: tags.into_iter().map(|(name,)| name).collect(),
            properties,
            created_at,
            updated_at,
            author,
        });
    }

    Ok(documents)
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
    let normalized = normalize_dataview_key(field);
    match normalized.as_str() {
        "title" => vec![document.title.clone()],
        "content" | "body" => vec![document.content.clone()],
        "space" | "spacename" => vec![document.space_name.clone(), document.space_id.clone()],
        "spaceid" => vec![document.space_id.clone()],
        "tag" | "tags" => document.tags.clone(),
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

async fn matches_filters(
    state: &AppState,
    document_id: &str,
    title: Option<&str>,
    folder: Option<&str>,
    tag: Option<&str>,
    property_name: Option<&str>,
    property_value: Option<&str>,
    has_attachment: Option<bool>,
    has_cover: Option<bool>,
    has_workflow: Option<bool>,
    has_tag: Option<bool>,
    has_property: Option<bool>,
    is_locked: Option<bool>,
    created_on: Option<&str>,
    updated_on: Option<&str>,
) -> Result<bool, AppError> {
    if let Some(title_filter) = title {
        let title_match: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM documents WHERE id = ?1 AND lower(title) LIKE lower(?2) LIMIT 1",
        )
        .bind(document_id)
        .bind(format!("%{}%", title_filter))
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;

        if title_match.is_none() {
            return Ok(false);
        }
    }

    if let Some(folder_name_or_id) = folder {
        let folder_match: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM documents d \
             JOIN folders f ON f.id = d.folder_id \
             WHERE d.id = ?1 AND (f.id = ?2 OR lower(f.name) = lower(?2)) \
             LIMIT 1",
        )
        .bind(document_id)
        .bind(folder_name_or_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;

        if folder_match.is_none() {
            return Ok(false);
        }
    }

    if let Some(tag_name) = tag {
        let tag_match: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM tags t \
             JOIN doc_tags dt ON t.id = dt.tag_id \
             WHERE dt.document_id = ?1 AND lower(t.name) = lower(?2) \
             LIMIT 1",
        )
        .bind(document_id)
        .bind(tag_name)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;

        if tag_match.is_none() {
            return Ok(false);
        }
    }

    if property_name.is_some() || property_value.is_some() {
        let property_match: Option<(i64,)> = match (property_name, property_value) {
            (Some(name), Some(value)) => sqlx::query_as(
                "SELECT 1 FROM property_values pv \
                 JOIN properties_schema ps ON pv.property_id = ps.id \
                 WHERE pv.document_id = ?1 AND lower(ps.name) = lower(?2) \
                 AND lower(COALESCE(pv.value, '')) LIKE lower(?3) \
                 LIMIT 1",
            )
            .bind(document_id)
            .bind(name)
            .bind(format!("%{}%", value))
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?,
            (Some(name), None) => sqlx::query_as(
                "SELECT 1 FROM property_values pv \
                 JOIN properties_schema ps ON pv.property_id = ps.id \
                 WHERE pv.document_id = ?1 AND lower(ps.name) = lower(?2) \
                 LIMIT 1",
            )
            .bind(document_id)
            .bind(name)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?,
            (None, Some(value)) => sqlx::query_as(
                "SELECT 1 FROM property_values pv \
                 WHERE pv.document_id = ?1 AND lower(COALESCE(pv.value, '')) LIKE lower(?2) \
                 LIMIT 1",
            )
            .bind(document_id)
            .bind(format!("%{}%", value))
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?,
            (None, None) => None,
        };

        if property_match.is_none() {
            return Ok(false);
        }
    }

    if let Some(expected) = has_attachment {
        let attachment_exists: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM attachments WHERE document_id = ?1 LIMIT 1")
                .bind(document_id)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        if attachment_exists.is_some() != expected {
            return Ok(false);
        }
    }

    if let Some(expected) = has_cover {
        let cover_exists: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM documents WHERE id = ?1 AND cover_image IS NOT NULL AND trim(cover_image) != '' LIMIT 1",
        )
        .bind(document_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;
        if cover_exists.is_some() != expected {
            return Ok(false);
        }
    }

    if let Some(expected) = has_workflow {
        let workflow_exists: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM document_workflows WHERE document_id = ?1 LIMIT 1")
                .bind(document_id)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        if workflow_exists.is_some() != expected {
            return Ok(false);
        }
    }

    if let Some(expected) = has_tag {
        let tag_exists: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM doc_tags WHERE document_id = ?1 LIMIT 1")
                .bind(document_id)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        if tag_exists.is_some() != expected {
            return Ok(false);
        }
    }

    if let Some(expected) = has_property {
        let property_exists: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM property_values WHERE document_id = ?1 LIMIT 1")
                .bind(document_id)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        if property_exists.is_some() != expected {
            return Ok(false);
        }
    }

    if let Some(expected) = is_locked {
        let locked: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM documents WHERE id = ?1 AND locked = ?2 LIMIT 1")
                .bind(document_id)
                .bind(if expected { 1 } else { 0 })
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        if locked.is_none() {
            return Ok(false);
        }
    }

    if let Some(date) = created_on.filter(|date| is_valid_search_date(Some(date))) {
        let created_match: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM documents WHERE id = ?1 AND created_at LIKE ?2 LIMIT 1")
                .bind(document_id)
                .bind(format!("{}%", date))
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        if created_match.is_none() {
            return Ok(false);
        }
    }

    if let Some(date) = updated_on.filter(|date| is_valid_search_date(Some(date))) {
        let updated_match: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM documents WHERE id = ?1 AND updated_at LIKE ?2 LIMIT 1")
                .bind(document_id)
                .bind(format!("{}%", date))
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        if updated_match.is_none() {
            return Ok(false);
        }
    }

    Ok(true)
}

fn is_valid_search_date(value: Option<&str>) -> bool {
    value.is_some_and(|date| {
        date.len() == 10
            && date.chars().enumerate().all(|(index, ch)| {
                matches!(index, 4 | 7) == (ch == '-') && (ch.is_ascii_digit() || ch == '-')
            })
    })
}

async fn can_read_search_document(
    state: &AppState,
    document_id: &str,
    user_id: &str,
) -> Result<bool, AppError> {
    let readable: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM documents d \
         WHERE d.id = ?1 AND (\
             EXISTS (SELECT 1 FROM users u WHERE u.id = ?2 AND u.role = 'admin') \
             OR EXISTS (SELECT 1 FROM space_members sm WHERE sm.space_id = d.space_id AND sm.user_id = ?2) \
             OR EXISTS (\
                 SELECT 1 FROM space_group_permissions sgp \
                 JOIN group_members gm ON gm.group_id = sgp.group_id \
                 WHERE sgp.space_id = d.space_id AND gm.user_id = ?2\
             )\
         ) LIMIT 1",
    )
    .bind(document_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(readable.is_some())
}

/// POST /api/v1/admin/search/reindex — rebuild the entire search index
async fn reindex(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    // Fetch all documents
    let docs: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT id, space_id, title, content_path FROM documents")
            .fetch_all(&state.db)
            .await
            .map_err(AppError::internal)?;

    // Fetch tags for each doc
    let mut doc_entries = Vec::new();
    for (id, space_id, title, content_path) in docs {
        let body = crate::storage::read_document(&content_path)
            .await
            .unwrap_or_default();

        let tags: Vec<(String,)> = sqlx::query_as(
            "SELECT t.name FROM tags t JOIN doc_tags dt ON t.id = dt.tag_id WHERE dt.document_id = ?1",
        )
        .bind(&id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

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

    async fn search_test_state() -> AppState {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        for statement in [
            "CREATE TABLE users (id TEXT PRIMARY KEY, role TEXT NOT NULL)",
            "CREATE TABLE documents (id TEXT PRIMARY KEY, space_id TEXT NOT NULL, title TEXT NOT NULL DEFAULT '')",
            "CREATE TABLE space_members (space_id TEXT NOT NULL, user_id TEXT NOT NULL)",
            "CREATE TABLE space_group_permissions (space_id TEXT NOT NULL, group_id TEXT NOT NULL)",
            "CREATE TABLE group_members (group_id TEXT NOT NULL, user_id TEXT NOT NULL)",
        ] {
            sqlx::query(statement)
                .execute(&db)
                .await
                .expect("create search fixture table");
        }

        let index_path = std::env::temp_dir().join(format!(
            "starlane-notes-search-test-{}",
            uuid::Uuid::new_v4()
        ));
        AppState {
            db,
            jwt_secret: "test-secret-with-enough-length-for-jwt".to_string(),
            base_url: "http://localhost:3000".to_string(),
            search_index: crate::search::SearchIndex::new(index_path).expect("search index"),
            oauth_cache: crate::auth::routes::OAuthStateCache::new(),
        }
    }

    #[tokio::test]
    async fn search_title_filter_matches_document_title_metadata() {
        let state = search_test_state().await;
        sqlx::query("INSERT INTO documents (id, space_id, title) VALUES ('doc-a', 'space-a', 'Launch Plan')")
            .execute(&state.db)
            .await
            .expect("seed matching document");
        sqlx::query("INSERT INTO documents (id, space_id, title) VALUES ('doc-b', 'space-a', 'Retrospective')")
            .execute(&state.db)
            .await
            .expect("seed non-matching document");

        assert!(matches_filters(
            &state,
            "doc-a",
            Some("launch"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("matching title filter"));
        assert!(!matches_filters(
            &state,
            "doc-b",
            Some("launch"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("non-matching title filter"));
    }

    fn doc() -> DataviewDocument {
        DataviewDocument {
            id: "doc-a".to_string(),
            title: "Project Alpha Plan".to_string(),
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
        }
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
    fn dataview_result_limit_defaults_to_documented_api_limit_and_caps_large_requests() {
        assert_eq!(dataview_result_limit(None), 50);
        assert_eq!(dataview_result_limit(Some(0)), 0);
        assert_eq!(dataview_result_limit(Some(12)), 12);
        assert_eq!(dataview_result_limit(Some(250)), 100);
    }

    #[tokio::test]
    async fn search_read_boundary_allows_direct_group_and_admin_access_only() {
        let state = search_test_state().await;
        for (id, role) in [
            ("direct-user", "member"),
            ("group-user", "member"),
            ("admin-user", "admin"),
            ("outside-user", "member"),
        ] {
            sqlx::query("INSERT INTO users (id, role) VALUES (?1, ?2)")
                .bind(id)
                .bind(role)
                .execute(&state.db)
                .await
                .expect("seed user");
        }
        sqlx::query("INSERT INTO documents (id, space_id) VALUES ('doc-a', 'space-a')")
            .execute(&state.db)
            .await
            .expect("seed document");
        sqlx::query(
            "INSERT INTO space_members (space_id, user_id) VALUES ('space-a', 'direct-user')",
        )
        .execute(&state.db)
        .await
        .expect("seed direct member");
        sqlx::query("INSERT INTO space_group_permissions (space_id, group_id) VALUES ('space-a', 'group-a')")
            .execute(&state.db)
            .await
            .expect("seed group permission");
        sqlx::query(
            "INSERT INTO group_members (group_id, user_id) VALUES ('group-a', 'group-user')",
        )
        .execute(&state.db)
        .await
        .expect("seed group member");

        assert!(can_read_search_document(&state, "doc-a", "direct-user")
            .await
            .unwrap());
        assert!(can_read_search_document(&state, "doc-a", "group-user")
            .await
            .unwrap());
        assert!(can_read_search_document(&state, "doc-a", "admin-user")
            .await
            .unwrap());
        assert!(!can_read_search_document(&state, "doc-a", "outside-user")
            .await
            .unwrap());
    }
}

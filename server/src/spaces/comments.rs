use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/spaces/{space_id}/documents/{doc_id}/comments",
            get(list_comments).post(create_comment),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}",
            axum::routing::put(update_comment).delete(delete_comment),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}/resolve",
            axum::routing::post(resolve_comment),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/reactions",
            get(list_reactions).post(toggle_reaction),
        )
}

#[derive(Deserialize)]
pub struct CreateCommentRequest {
    pub content: String,
    pub parent_id: Option<String>,
    pub selection_start: Option<i64>,
    pub selection_end: Option<i64>,
    pub selection_text: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateCommentRequest {
    pub content: String,
}

#[derive(Serialize)]
pub struct CommentResponse {
    pub id: String,
    pub document_id: String,
    pub parent_id: Option<String>,
    pub user_id: String,
    pub user_name: String,
    pub content: String,
    pub selection_start: Option<i64>,
    pub selection_end: Option<i64>,
    pub selection_text: Option<String>,
    pub resolved: bool,
    pub resolved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub replies: Vec<CommentResponse>,
}

#[derive(Deserialize)]
pub struct ToggleReactionRequest {
    pub emoji: String,
}

#[derive(Serialize)]
pub struct ReactionGroup {
    pub emoji: String,
    pub count: i64,
    pub user_reacted: bool,
}

async fn list_comments(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;
    require_document_in_space(&state, &space_id, &doc_id).await?;

    // Fetch top-level comments
    let comments: Vec<(
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        Option<i64>,
        Option<i64>,
        Option<String>,
        i32,
        Option<String>,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT c.id, c.document_id, c.parent_id, c.user_id, u.display_name, c.content, \
             c.selection_start, c.selection_end, c.selection_text, c.resolved, c.resolved_by, \
             c.created_at, c.updated_at \
             FROM comments c JOIN users u ON c.user_id = u.id \
             WHERE c.document_id = ?1 AND c.parent_id IS NULL \
             ORDER BY c.created_at",
    )
    .bind(&doc_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    // Fetch all replies
    let replies: Vec<(
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        Option<i64>,
        Option<i64>,
        Option<String>,
        i32,
        Option<String>,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT c.id, c.document_id, c.parent_id, c.user_id, u.display_name, c.content, \
             c.selection_start, c.selection_end, c.selection_text, c.resolved, c.resolved_by, \
             c.created_at, c.updated_at \
             FROM comments c JOIN users u ON c.user_id = u.id \
             WHERE c.document_id = ?1 AND c.parent_id IS NOT NULL \
             ORDER BY c.created_at",
    )
    .bind(&doc_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<CommentResponse> = comments
        .into_iter()
        .map(
            |(id, did, pid, uid, uname, content, ss, se, st, resolved, rb, created, updated)| {
                let thread_replies: Vec<CommentResponse> = replies
                    .iter()
                    .filter(|r| r.2.as_deref() == Some(&id))
                    .map(|r| CommentResponse {
                        id: r.0.clone(),
                        document_id: r.1.clone(),
                        parent_id: r.2.clone(),
                        user_id: r.3.clone(),
                        user_name: r.4.clone(),
                        content: r.5.clone(),
                        selection_start: r.6,
                        selection_end: r.7,
                        selection_text: r.8.clone(),
                        resolved: r.9 != 0,
                        resolved_by: r.10.clone(),
                        created_at: r.11.clone(),
                        updated_at: r.12.clone(),
                        replies: vec![],
                    })
                    .collect();

                CommentResponse {
                    id,
                    document_id: did,
                    parent_id: pid,
                    user_id: uid,
                    user_name: uname,
                    content,
                    selection_start: ss,
                    selection_end: se,
                    selection_text: st,
                    resolved: resolved != 0,
                    resolved_by: rb,
                    created_at: created,
                    updated_at: updated,
                    replies: thread_replies,
                }
            },
        )
        .collect();

    Ok(Json(result))
}

async fn create_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<CreateCommentRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_comment_access(&state, &space_id, &doc_id, &auth.claims.sub).await?;

    if req.content.is_empty() {
        return Err(AppError::BadRequest("Comment body is required".into()));
    }

    if req.content.len() > 10_000 {
        return Err(AppError::BadRequest(
            "Comment body must be at most 10000 characters".into(),
        ));
    }

    if let Some(parent_id) = &req.parent_id {
        require_comment_in_document(&state, parent_id, &doc_id).await?;
    }

    let comment_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO comments (id, document_id, parent_id, user_id, content, selection_start, selection_end, selection_text) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&comment_id)
    .bind(&doc_id)
    .bind(&req.parent_id)
    .bind(&auth.claims.sub)
    .bind(&req.content)
    .bind(req.selection_start)
    .bind(req.selection_end)
    .bind(&req.selection_text)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    // Auto-subscribe commenter
    sqlx::query(
        "INSERT INTO doc_subscriptions (document_id, user_id) VALUES (?1, ?2) \
         ON CONFLICT(document_id, user_id) DO NOTHING",
    )
    .bind(&doc_id)
    .bind(&auth.claims.sub)
    .execute(&state.db)
    .await
    .ok();

    if let Err(error) = notify_comment_recipients(
        &state,
        &doc_id,
        req.parent_id.as_deref(),
        &auth.claims.sub,
        &req.content,
    )
    .await
    {
        tracing::warn!("Failed to notify comment recipients: {}", error);
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": comment_id })),
    ))
}

async fn notify_comment_recipients(
    state: &AppState,
    document_id: &str,
    parent_id: Option<&str>,
    actor_id: &str,
    content: &str,
) -> Result<(), sqlx::Error> {
    let document_title =
        sqlx::query_as::<_, (String,)>("SELECT title FROM documents WHERE id = ?1")
            .bind(document_id)
            .fetch_optional(&state.db)
            .await?
            .map(|row| row.0)
            .unwrap_or_else(|| "Untitled".to_string());

    let subscribers: Vec<(String,)> = sqlx::query_as(
        "SELECT ds.user_id FROM doc_subscriptions ds \
         JOIN users u ON u.id = ds.user_id \
         WHERE ds.document_id = ?1 AND u.is_active = 1",
    )
    .bind(document_id)
    .fetch_all(&state.db)
    .await?;
    let parent_author = match parent_id {
        Some(parent_id) => sqlx::query_as::<_, (String,)>(
            "SELECT c.user_id FROM comments c JOIN users u ON u.id = c.user_id \
             WHERE c.id = ?1 AND c.document_id = ?2 AND u.is_active = 1",
        )
        .bind(parent_id)
        .bind(document_id)
        .fetch_optional(&state.db)
        .await?
        .map(|row| row.0),
        None => None,
    };

    let body = content.chars().take(240).collect::<String>();
    for (recipient_id, notification_type) in comment_notification_recipients(
        subscribers.into_iter().map(|row| row.0),
        parent_author.as_deref(),
        actor_id,
    ) {
        let is_reply = notification_type == "comment_reply";
        crate::notifications::notify(
            &state.db,
            &recipient_id,
            notification_type,
            &format!(
                "{} on {}",
                if is_reply { "New reply" } else { "New comment" },
                document_title
            ),
            Some(&body),
            Some(actor_id),
            Some("document"),
            Some(document_id),
        )
        .await;
    }

    Ok(())
}

fn comment_notification_type(is_reply: bool) -> &'static str {
    if is_reply {
        "comment_reply"
    } else {
        "comment"
    }
}

fn comment_notification_recipients(
    subscribers: impl IntoIterator<Item = String>,
    parent_author: Option<&str>,
    actor_id: &str,
) -> Vec<(String, &'static str)> {
    let mut recipient_ids: BTreeSet<String> = subscribers.into_iter().collect();
    if let Some(parent_author) = parent_author {
        recipient_ids.insert(parent_author.to_string());
    }

    recipient_ids
        .into_iter()
        .filter(|recipient_id| recipient_id != actor_id)
        .map(|recipient_id| {
            let is_reply = parent_author == Some(recipient_id.as_str());
            (recipient_id, comment_notification_type(is_reply))
        })
        .collect()
}

async fn update_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, comment_id)): Path<(String, String, String)>,
    Json(req): Json<UpdateCommentRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_comment_access(&state, &space_id, &doc_id, &auth.claims.sub).await?;
    require_comment_in_document(&state, &comment_id, &doc_id).await?;

    if req.content.is_empty() {
        return Err(AppError::BadRequest("Comment body is required".into()));
    }

    if req.content.len() > 10_000 {
        return Err(AppError::BadRequest(
            "Comment body must be at most 10000 characters".into(),
        ));
    }

    sqlx::query("UPDATE comments SET content = ?1, updated_at = ?2 WHERE id = ?3 AND user_id = ?4")
        .bind(&req.content)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&comment_id)
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, comment_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    require_comment_access(&state, &space_id, &doc_id, &auth.claims.sub).await?;
    require_comment_in_document(&state, &comment_id, &doc_id).await?;

    sqlx::query("DELETE FROM comments WHERE id = ?1 AND user_id = ?2")
        .bind(&comment_id)
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn resolve_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, comment_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    require_comment_access(&state, &space_id, &doc_id, &auth.claims.sub).await?;
    require_comment_in_document(&state, &comment_id, &doc_id).await?;

    sqlx::query(
        "UPDATE comments SET resolved = 1, resolved_by = ?1, \
         resolved_at = ?2 WHERE id = ?3",
    )
    .bind(&auth.claims.sub)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&comment_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_reactions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;
    require_document_in_space(&state, &space_id, &doc_id).await?;

    let reactions: Vec<(String, i64)> = sqlx::query_as(
        "SELECT emoji, COUNT(*) as count FROM reactions WHERE document_id = ?1 GROUP BY emoji",
    )
    .bind(&doc_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let user_reactions: Vec<(String,)> =
        sqlx::query_as("SELECT emoji FROM reactions WHERE document_id = ?1 AND user_id = ?2")
            .bind(&doc_id)
            .bind(&auth.claims.sub)
            .fetch_all(&state.db)
            .await
            .map_err(AppError::internal)?;

    let user_emojis: Vec<String> = user_reactions.into_iter().map(|r| r.0).collect();

    let result: Vec<ReactionGroup> = reactions
        .into_iter()
        .map(|(emoji, count)| ReactionGroup {
            user_reacted: user_emojis.contains(&emoji),
            emoji,
            count,
        })
        .collect();

    Ok(Json(result))
}

async fn toggle_reaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<ToggleReactionRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;
    require_document_in_space(&state, &space_id, &doc_id).await?;

    if !is_valid_reaction_emoji(&req.emoji) {
        return Err(AppError::BadRequest(
            "Reaction emoji must be 1 to 32 characters".into(),
        ));
    }

    // Check if reaction exists
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM reactions WHERE document_id = ?1 AND user_id = ?2 AND emoji = ?3",
    )
    .bind(&doc_id)
    .bind(&auth.claims.sub)
    .bind(&req.emoji)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    if let Some((id,)) = existing {
        // Remove reaction
        sqlx::query("DELETE FROM reactions WHERE id = ?1")
            .bind(&id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    } else {
        // Add reaction
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO reactions (id, document_id, user_id, emoji) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(&id)
        .bind(&doc_id)
        .bind(&auth.claims.sub)
        .bind(&req.emoji)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn require_document_in_space(
    state: &AppState,
    space_id: &str,
    document_id: &str,
) -> Result<(), AppError> {
    let found: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM documents WHERE id = ?1 AND space_id = ?2 LIMIT 1")
            .bind(document_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    if found.is_none() {
        return Err(AppError::NotFound(
            "Document not found in this space".into(),
        ));
    }
    Ok(())
}

async fn require_comment_access(
    state: &AppState,
    space_id: &str,
    document_id: &str,
    user_id: &str,
) -> Result<(), AppError> {
    super::routes::require_space_role(state, space_id, user_id, &["owner", "editor", "viewer"])
        .await?;
    require_document_in_space(state, space_id, document_id).await
}

async fn require_comment_in_document(
    state: &AppState,
    comment_id: &str,
    document_id: &str,
) -> Result<(), AppError> {
    let found: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM comments WHERE id = ?1 AND document_id = ?2 LIMIT 1")
            .bind(comment_id)
            .bind(document_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    if found.is_none() {
        return Err(AppError::NotFound(
            "Comment not found in this document".into(),
        ));
    }
    Ok(())
}

fn is_valid_reaction_emoji(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 32
}

#[cfg(test)]
mod tests {
    use super::{
        comment_notification_recipients, comment_notification_type, is_valid_reaction_emoji,
        require_comment_in_document,
    };
    use crate::auth::routes::AppError;
    use crate::AppState;
    use sqlx::{any::AnyPoolOptions, AnyPool};

    async fn comment_test_state() -> AppState {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        create_comment_fixture_tables(&db).await;

        let index_path = std::env::temp_dir().join(format!(
            "starlane-notes-comment-test-{}",
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

    async fn create_comment_fixture_tables(db: &AnyPool) {
        for statement in
            ["CREATE TABLE comments (id TEXT PRIMARY KEY NOT NULL, document_id TEXT NOT NULL)"]
        {
            sqlx::query(statement)
                .execute(db)
                .await
                .expect("create comment fixture table");
        }
    }

    #[test]
    fn reaction_emoji_is_bounded_and_non_empty() {
        assert!(is_valid_reaction_emoji("👍"));
        assert!(!is_valid_reaction_emoji("   "));
        assert!(!is_valid_reaction_emoji(&"a".repeat(33)));
    }

    #[test]
    fn parent_commenters_receive_reply_notification_types() {
        assert_eq!(comment_notification_type(false), "comment");
        assert_eq!(comment_notification_type(true), "comment_reply");
    }

    #[test]
    fn comment_notifications_deduplicate_recipients_and_skip_the_actor() {
        let recipients = comment_notification_recipients(
            ["alice".to_string(), "bob".to_string(), "bob".to_string()],
            Some("carol"),
            "alice",
        );
        assert_eq!(
            recipients,
            vec![
                ("bob".to_string(), "comment"),
                ("carol".to_string(), "comment_reply"),
            ]
        );
    }

    #[tokio::test]
    async fn comment_must_belong_to_the_routed_document() {
        let state = comment_test_state().await;
        sqlx::query("INSERT INTO comments (id, document_id) VALUES (?1, ?2)")
            .bind("comment-a")
            .bind("document-a")
            .execute(&state.db)
            .await
            .expect("seed comment");

        require_comment_in_document(&state, "comment-a", "document-a")
            .await
            .expect("matching comment is accepted");

        let err = require_comment_in_document(&state, "comment-a", "document-b")
            .await
            .expect_err("cross-document comment must be rejected");
        assert!(matches!(err, AppError::NotFound(_)));
    }
}

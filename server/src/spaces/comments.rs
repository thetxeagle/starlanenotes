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
    list_comments_current(
        State(state),
        auth,
        Path((space_id, doc_id)),
        #[cfg(test)]
        None,
    )
    .await
}

async fn list_comments_current(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    #[cfg(test)] after_load: Option<super::documents::NotificationTitleReadHook>,
) -> Result<impl IntoResponse, AppError> {
    super::access::principal_from_auth(&auth)?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    check_page(
        &mut conn,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::View,
    )
    .await?;

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
             WHERE c.document_id = $1 AND c.parent_id IS NULL \
             ORDER BY c.created_at",
    )
    .bind(&doc_id)
    .fetch_all(&mut *conn)
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
             WHERE c.document_id = $1 AND c.parent_id IS NOT NULL \
             ORDER BY c.created_at",
    )
    .bind(&doc_id)
    .fetch_all(&mut *conn)
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

    #[cfg(test)]
    if let Some(hook) = after_load {
        hook().await;
    }
    check_page(
        &mut conn,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::View,
    )
    .await?;
    super::access::require_current_human_read(&mut conn, &auth).await?;
    Ok(Json(result))
}

async fn notify_comment_recipients(
    state: &AppState,
    space_id: &str,
    document_id: &str,
    parent_id: Option<&str>,
    actor_id: &str,
    content: &str,
) -> Result<(), sqlx::Error> {
    let subscribers: Vec<(String,)> = sqlx::query_as(
        "SELECT ds.user_id FROM doc_subscriptions ds \
         JOIN users u ON u.id = ds.user_id \
         WHERE ds.document_id = $1 AND u.is_active = 1",
    )
    .bind(document_id)
    .fetch_all(&state.db)
    .await?;
    let parent_author = match parent_id {
        Some(parent_id) => sqlx::query_as::<_, (String,)>(
            "SELECT c.user_id FROM comments c JOIN users u ON u.id = c.user_id \
             WHERE c.id = $1 AND c.document_id = $2 AND u.is_active = 1",
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
        let mut conn = state.db.acquire().await?;
        let Some((_, document_title)) = super::documents::readable_notification_page_title(
            &mut conn,
            space_id,
            document_id,
            &recipient_id,
        )
        .await
        .map_err(|error| sqlx::Error::Protocol(format!("{error:?}")))?
        else {
            continue;
        };
        drop(conn);
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

async fn list_reactions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    list_reactions_current(
        State(state),
        auth,
        Path((space_id, doc_id)),
        #[cfg(test)]
        None,
    )
    .await
}

async fn list_reactions_current(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    #[cfg(test)] after_load: Option<super::documents::NotificationTitleReadHook>,
) -> Result<impl IntoResponse, AppError> {
    super::access::principal_from_auth(&auth)?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    check_page(
        &mut conn,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::View,
    )
    .await?;

    let reactions: Vec<(String, i64)> = sqlx::query_as(
        "SELECT emoji, COUNT(*) as count FROM reactions WHERE document_id = $1 GROUP BY emoji",
    )
    .bind(&doc_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    let user_reactions: Vec<(String,)> =
        sqlx::query_as("SELECT emoji FROM reactions WHERE document_id = $1 AND user_id = $2")
            .bind(&doc_id)
            .bind(&auth.claims.sub)
            .fetch_all(&mut *conn)
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

    #[cfg(test)]
    if let Some(hook) = after_load {
        hook().await;
    }
    check_page(
        &mut conn,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::View,
    )
    .await?;
    super::access::require_current_human_read(&mut conn, &auth).await?;
    Ok(Json(result))
}

async fn check_page(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
    doc_id: &str,
    operation: super::access::PageOperation,
) -> Result<(), AppError> {
    let principal = super::access::principal_from_auth(auth)?;
    super::access::resolve_page_access(
        conn,
        principal,
        space_id,
        doc_id,
        operation,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}
async fn begin_comment_mutation<'a>(
    state: &'a AppState,
    auth: &AuthUser,
    space_id: &str,
    doc_id: &str,
) -> Result<sqlx::Transaction<'a, sqlx::Any>, AppError> {
    super::access::principal_from_auth(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space_id]).await?;
    check_page(
        &mut tx,
        auth,
        space_id,
        doc_id,
        super::access::PageOperation::Comment,
    )
    .await?;
    let row=sqlx::query("UPDATE documents SET title=title WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(doc_id).bind(space_id).execute(&mut *tx).await.map_err(AppError::internal)?;
    if row.rows_affected() != 1 {
        return Err(AppError::NotFound("Document unavailable".into()));
    }
    check_page(
        &mut tx,
        auth,
        space_id,
        doc_id,
        super::access::PageOperation::Comment,
    )
    .await?;
    Ok(tx)
}
async fn lock_comment(
    conn: &mut sqlx::AnyConnection,
    comment_id: &str,
    doc_id: &str,
) -> Result<String, AppError> {
    let row = sqlx::query("UPDATE comments SET content=content WHERE id=$1 AND document_id=$2")
        .bind(comment_id)
        .bind(doc_id)
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if row.rows_affected() != 1 {
        return Err(AppError::NotFound("Comment unavailable".into()));
    }
    sqlx::query_scalar("SELECT user_id FROM comments WHERE id=$1 AND document_id=$2")
        .bind(comment_id)
        .bind(doc_id)
        .fetch_one(conn)
        .await
        .map_err(AppError::internal)
}
async fn finish_comment_mutation(
    tx: sqlx::Transaction<'_, sqlx::Any>,
    auth: &AuthUser,
    space_id: &str,
    doc_id: &str,
) -> Result<(), AppError> {
    let mut tx = tx;
    check_page(
        &mut tx,
        auth,
        space_id,
        doc_id,
        super::access::PageOperation::Comment,
    )
    .await?;
    tx.commit().await.map_err(AppError::internal)
}
fn validate_comment_body(content: &str) -> Result<(), AppError> {
    if content.is_empty() {
        return Err(AppError::BadRequest("Comment body is required".into()));
    }
    if content.len() > 10_000 {
        return Err(AppError::BadRequest(
            "Comment body must be at most 10000 characters".into(),
        ));
    }
    Ok(())
}
async fn create_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<CreateCommentRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = begin_comment_mutation(&state, &auth, &space_id, &doc_id).await?;
    validate_comment_body(&req.content)?;
    if let Some(parent) = &req.parent_id {
        lock_comment(&mut tx, parent, &doc_id).await?;
    }
    check_page(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::Comment,
    )
    .await?;
    let id = Uuid::new_v4().to_string();
    let row=sqlx::query("INSERT INTO comments(id,document_id,parent_id,user_id,content,selection_start,selection_end,selection_text) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(&id).bind(&doc_id).bind(&req.parent_id).bind(&auth.claims.sub).bind(&req.content).bind(req.selection_start).bind(req.selection_end).bind(&req.selection_text).execute(&mut *tx).await.map_err(AppError::internal)?;
    if row.rows_affected() != 1 {
        return Err(AppError::Conflict("Comment write did not apply".into()));
    }
    sqlx::query("INSERT INTO doc_subscriptions(document_id,user_id) VALUES($1,$2) ON CONFLICT(document_id,user_id) DO NOTHING").bind(&doc_id).bind(&auth.claims.sub).execute(&mut *tx).await.map_err(AppError::internal)?;
    let created: Option<(String, Option<String>, String)> =
        sqlx::query_as("SELECT document_id,parent_id,user_id FROM comments WHERE id=$1")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if created
        != Some((
            doc_id.clone(),
            req.parent_id.clone(),
            auth.claims.sub.clone(),
        ))
    {
        return Err(AppError::Conflict("Comment target changed; retry".into()));
    }
    if let Some(parent) = &req.parent_id {
        lock_comment(&mut tx, parent, &doc_id).await?;
    }
    finish_comment_mutation(tx, &auth, &space_id, &doc_id).await?;
    if let Err(error) = notify_comment_recipients(
        &state,
        &space_id,
        &doc_id,
        req.parent_id.as_deref(),
        &auth.claims.sub,
        &req.content,
    )
    .await
    {
        tracing::warn!(%error,"Failed to notify comment recipients");
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id":id}))))
}
async fn update_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, comment_id)): Path<(String, String, String)>,
    Json(req): Json<UpdateCommentRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = begin_comment_mutation(&state, &auth, &space_id, &doc_id).await?;
    let author = lock_comment(&mut tx, &comment_id, &doc_id).await?;
    if author != auth.claims.sub {
        return Err(AppError::Forbidden(
            "Only the comment author can modify this comment".into(),
        ));
    }
    validate_comment_body(&req.content)?;
    check_page(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::Comment,
    )
    .await?;
    let row=sqlx::query("UPDATE comments SET content=$1,updated_at=$2 WHERE id=$3 AND document_id=$4 AND user_id=$5").bind(&req.content).bind(chrono::Utc::now().to_rfc3339()).bind(&comment_id).bind(&doc_id).bind(&auth.claims.sub).execute(&mut *tx).await.map_err(AppError::internal)?;
    if row.rows_affected() != 1 {
        return Err(AppError::NotFound("Comment unavailable".into()));
    }
    finish_comment_mutation(tx, &auth, &space_id, &doc_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn require_safe_comment_cascade(
    conn: &mut sqlx::AnyConnection,
    id: &str,
    doc: &str,
) -> Result<(), AppError> {
    let foreign:i64=sqlx::query_scalar("WITH RECURSIVE descendants(id,document_id) AS (SELECT id,document_id FROM comments WHERE id=$1 UNION SELECT c.id,c.document_id FROM comments c JOIN descendants d ON c.parent_id=d.id) SELECT count(*) FROM descendants WHERE document_id<>$2").bind(id).bind(doc).fetch_one(conn).await.map_err(AppError::internal)?;
    if foreign != 0 {
        return Err(AppError::Conflict(
            "Comment thread is inconsistent; repair its page associations before deletion".into(),
        ));
    }
    Ok(())
}
async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = begin_comment_mutation(&state, &auth, &space_id, &doc_id).await?;
    let author = lock_comment(&mut tx, &id, &doc_id).await?;
    if author != auth.claims.sub {
        return Err(AppError::Forbidden(
            "Only the comment author can delete this comment".into(),
        ));
    }
    require_safe_comment_cascade(&mut tx, &id, &doc_id).await?;
    check_page(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::Comment,
    )
    .await?;
    let row = sqlx::query("DELETE FROM comments WHERE id=$1 AND document_id=$2 AND user_id=$3")
        .bind(&id)
        .bind(&doc_id)
        .bind(&auth.claims.sub)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if row.rows_affected() != 1 {
        return Err(AppError::NotFound("Comment unavailable".into()));
    }
    finish_comment_mutation(tx, &auth, &space_id, &doc_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn resolve_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = begin_comment_mutation(&state, &auth, &space_id, &doc_id).await?;
    lock_comment(&mut tx, &id, &doc_id).await?;
    check_page(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::Comment,
    )
    .await?;
    let row=sqlx::query("UPDATE comments SET resolved=1,resolved_by=$1,resolved_at=$2 WHERE id=$3 AND document_id=$4").bind(&auth.claims.sub).bind(chrono::Utc::now().to_rfc3339()).bind(&id).bind(&doc_id).execute(&mut *tx).await.map_err(AppError::internal)?;
    if row.rows_affected() != 1 {
        return Err(AppError::NotFound("Comment unavailable".into()));
    }
    finish_comment_mutation(tx, &auth, &space_id, &doc_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn toggle_reaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<ToggleReactionRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = begin_comment_mutation(&state, &auth, &space_id, &doc_id).await?;
    if !is_valid_reaction_emoji(&req.emoji) {
        return Err(AppError::BadRequest(
            "Reaction emoji must be 1 to 32 characters".into(),
        ));
    }
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM reactions WHERE document_id=$1 AND user_id=$2 AND emoji=$3",
    )
    .bind(&doc_id)
    .bind(&auth.claims.sub)
    .bind(&req.emoji)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    let row = if let Some(id) = existing {
        sqlx::query("DELETE FROM reactions WHERE id=$1 AND document_id=$2 AND user_id=$3")
            .bind(&id)
            .bind(&doc_id)
            .bind(&auth.claims.sub)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
    } else {
        sqlx::query("INSERT INTO reactions(id,document_id,user_id,emoji) VALUES($1,$2,$3,$4)")
            .bind(Uuid::new_v4().to_string())
            .bind(&doc_id)
            .bind(&auth.claims.sub)
            .bind(&req.emoji)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
    };
    if row.rows_affected() != 1 {
        return Err(AppError::Conflict("Reaction write did not apply".into()));
    }
    finish_comment_mutation(tx, &auth, &space_id, &doc_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn is_valid_reaction_emoji(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 32
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    pub(super) async fn state() -> AppState {
        let state = crate::spaces::properties::tests::property_test_state().await;
        for user in ["owner-a", "editor-a", "viewer-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'hash','2999-01-01T00:00:00Z')").bind(format!("comment-{user}")).bind(user).execute(&state.db).await.unwrap();
        }
        state
    }
    fn auth(user: &str) -> AuthUser {
        let mut a = crate::spaces::properties::tests::test_auth(user);
        a.claims.sid = format!("comment-{user}");
        a
    }
    fn path() -> Path<(String, String)> {
        Path(("space-a".into(), "target-a".into()))
    }
    fn comment_path(id: &str) -> Path<(String, String, String)> {
        Path(("space-a".into(), "target-a".into(), id.into()))
    }
    fn body(parent: Option<&str>) -> Json<CreateCommentRequest> {
        Json(CreateCommentRequest {
            content: "Comment body".into(),
            parent_id: parent.map(str::to_owned),
            selection_start: Some(1),
            selection_end: Some(5),
            selection_text: Some("text".into()),
        })
    }
    pub(super) async fn seed_comment(
        state: &AppState,
        id: &str,
        doc: &str,
        user: &str,
        parent: Option<&str>,
    ) {
        sqlx::query("INSERT INTO comments(id,document_id,user_id,content,parent_id) VALUES($1,$2,$3,'Original',$4)").bind(id).bind(doc).bind(user).bind(parent).execute(&state.db).await.unwrap();
    }
    async fn private_draft(state: &AppState, owner: &str) {
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('private','target-a','space-a',$1,'Secret','Secret body','base')").bind(owner).execute(&state.db).await.unwrap();
    }
    async fn counts(state: &AppState) -> (i64, i64, i64) {
        sqlx::query_as("SELECT (SELECT count(*) FROM comments),(SELECT count(*) FROM reactions),(SELECT count(*) FROM doc_subscriptions)").fetch_one(&state.db).await.unwrap()
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
        assert_eq!(
            comment_notification_recipients(
                ["alice".into(), "bob".into(), "bob".into()],
                Some("carol"),
                "alice"
            ),
            vec![("bob".into(), "comment"), ("carol".into(), "comment_reply")]
        );
    }
    #[tokio::test]
    async fn comment_thread_bindings_and_author_rules_are_current_and_scoped() {
        let s = state().await;
        seed_comment(&s, "parent", "target-a", "editor-a", None).await;
        seed_comment(&s, "foreign", "target-b", "owner-a", None).await;
        assert!(create_comment(
            State(s.clone()),
            auth("editor-a"),
            path(),
            body(Some("foreign"))
        )
        .await
        .is_err());
        assert!(update_comment(
            State(s.clone()),
            auth("editor-a"),
            comment_path("foreign"),
            Json(UpdateCommentRequest {
                content: "X".into()
            })
        )
        .await
        .is_err());
        assert!(matches!(
            update_comment(
                State(s.clone()),
                auth("owner-a"),
                comment_path("parent"),
                Json(UpdateCommentRequest {
                    content: "X".into()
                })
            )
            .await,
            Err(AppError::Forbidden(_))
        ));
        assert!(
            delete_comment(State(s.clone()), auth("owner-a"), comment_path("parent"))
                .await
                .is_err()
        );
        let r = create_comment(
            State(s.clone()),
            auth("editor-a"),
            path(),
            body(Some("parent")),
        )
        .await
        .unwrap()
        .into_response();
        assert_eq!(r.status(), StatusCode::CREATED);
        assert!(
            resolve_comment(State(s.clone()), auth("owner-a"), comment_path("parent"))
                .await
                .is_ok()
        );
        assert!(update_comment(
            State(s.clone()),
            auth("editor-a"),
            comment_path("parent"),
            Json(UpdateCommentRequest {
                content: "Edited".into()
            })
        )
        .await
        .is_ok());
        let row: (String, i64, Option<String>) =
            sqlx::query_as("SELECT content,resolved,resolved_by FROM comments WHERE id='parent'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(row, ("Edited".into(), 1, Some("owner-a".into())));
        let response = list_comments(State(s.clone()), auth("viewer-a"), path())
            .await
            .unwrap()
            .into_response();
        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(json[0]["replies"][0]["selection_text"], "text");
        assert!(
            delete_comment(State(s.clone()), auth("editor-a"), comment_path("parent"))
                .await
                .is_ok()
        );
        assert_eq!(counts(&s).await.0, 1);
    }
    #[tokio::test]
    async fn private_comment_reads_and_writes_follow_participation_not_historical_author() {
        let s = state().await;
        private_draft(&s, "viewer-a").await;
        seed_comment(&s, "historical", "target-a", "editor-a", None).await;
        assert!(list_comments(State(s.clone()), auth("editor-a"), path())
            .await
            .is_err());
        assert!(list_reactions(State(s.clone()), auth("editor-a"), path())
            .await
            .is_err());
        assert!(update_comment(
            State(s.clone()),
            auth("editor-a"),
            comment_path("historical"),
            Json(UpdateCommentRequest {
                content: "X".into()
            })
        )
        .await
        .is_err());
        assert!(
            create_comment(State(s.clone()), auth("owner-a"), path(), body(None))
                .await
                .is_err()
        );
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('private','editor-a','viewer','viewer-a')").execute(&s.db).await.unwrap();
        assert!(list_comments(State(s.clone()), auth("editor-a"), path())
            .await
            .is_ok());
        assert!(
            create_comment(State(s.clone()), auth("editor-a"), path(), body(None))
                .await
                .is_ok()
        );
        sqlx::query("DELETE FROM document_draft_access WHERE user_id='editor-a'")
            .execute(&s.db)
            .await
            .unwrap();
        assert!(delete_comment(
            State(s.clone()),
            auth("editor-a"),
            comment_path("historical")
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn comment_role_and_source_lock_follow_existing_operation_policy() {
        let s = state().await;
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'")
            .execute(&s.db)
            .await
            .unwrap();
        assert!(
            create_comment(State(s.clone()), auth("editor-a"), path(), body(None))
                .await
                .is_ok()
        );
        assert!(
            create_comment(State(s.clone()), auth("viewer-a"), path(), body(None))
                .await
                .is_err()
        );
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('custom:comment','space-a','Comment','[\"view\",\"comment\"]','owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom:comment' WHERE user_id='viewer-a'")
            .execute(&s.db)
            .await
            .unwrap();
        assert!(
            create_comment(State(s.clone()), auth("viewer-a"), path(), body(None))
                .await
                .is_ok()
        );
        for _ in 0..2 {
            assert!(toggle_reaction(
                State(s.clone()),
                auth("viewer-a"),
                path(),
                Json(ToggleReactionRequest {
                    emoji: "👍".into()
                })
            )
            .await
            .is_ok());
        }
        assert_eq!(counts(&s).await.1, 0);
    }
    #[tokio::test]
    async fn comment_lists_conceal_foreign_missing_private_and_lifecycle_targets() {
        let s = state().await;
        private_draft(&s, "viewer-a").await;
        for operation in [0, 1] {
            let mut messages = Vec::new();
            for doc in ["target-a", "target-b", "missing"] {
                let p = Path(("space-a".into(), doc.into()));
                let err = if operation == 0 {
                    list_comments(State(s.clone()), auth("editor-a"), p)
                        .await
                        .err()
                        .unwrap()
                } else {
                    list_reactions(State(s.clone()), auth("editor-a"), p)
                        .await
                        .err()
                        .unwrap()
                };
                messages.push(format!("{err:?}"));
            }
            assert!(messages.iter().all(|m| m == &messages[0]));
        }
        for sql in [
            "UPDATE documents SET archived_at='archive' WHERE id='target-a'",
            "UPDATE documents SET deleted_at='trash' WHERE id='target-a'",
            "UPDATE documents SET parent_id='target-b' WHERE id='target-a'",
        ] {
            let s = state().await;
            sqlx::query(sql).execute(&s.db).await.unwrap();
            assert!(list_comments(State(s.clone()), auth("editor-a"), path())
                .await
                .is_err());
            assert!(
                create_comment(State(s.clone()), auth("editor-a"), path(), body(None))
                    .await
                    .is_err()
            );
            assert_eq!(counts(&s).await, (0, 0, 0));
        }
    }
    #[tokio::test]
    async fn comment_current_auth_checks_revoke_sessions_activity_and_key_ceiling() {
        for sql in [
            "DELETE FROM sessions WHERE user_id='editor-a'",
            "UPDATE users SET is_active=0 WHERE id='editor-a'",
            "DELETE FROM space_members WHERE user_id='editor-a'",
        ] {
            let s = state().await;
            sqlx::query(sql).execute(&s.db).await.unwrap();
            assert!(list_comments(State(s.clone()), auth("editor-a"), path())
                .await
                .is_err());
            assert!(
                create_comment(State(s.clone()), auth("editor-a"), path(), body(None))
                    .await
                    .is_err()
            );
            assert_eq!(counts(&s).await, (0, 0, 0));
        }
        let s = state().await;
        let mut expired = auth("editor-a");
        expired.claims.exp = 0;
        assert!(
            create_comment(State(s.clone()), expired, path(), body(None))
                .await
                .is_err()
        );
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('comment-key','editor-a','Key','hash','prefix',0,'[\"read\"]')").execute(&s.db).await.unwrap();
        let keyauth = || {
            let mut a = auth("editor-a");
            a.claims.sid = "api-key:comment-key".into();
            a.api_key_scopes = Some(vec!["admin".into()]);
            a
        };
        assert!(list_comments(State(s.clone()), keyauth(), path())
            .await
            .is_ok());
        assert!(
            create_comment(State(s.clone()), keyauth(), path(), body(None))
                .await
                .is_err()
        );
        sqlx::query("UPDATE api_keys SET scopes='[\"read\",\"write\"]'")
            .execute(&s.db)
            .await
            .unwrap();
        assert!(
            create_comment(State(s.clone()), keyauth(), path(), body(None))
                .await
                .is_ok()
        );
    }
    #[tokio::test]
    async fn comment_late_write_revocation_rolls_back_content_subscription_and_reaction() {
        for trigger in ["CREATE TRIGGER revoke AFTER INSERT ON comments BEGIN UPDATE users SET is_active=0 WHERE id=NEW.user_id; END","CREATE TRIGGER revoke AFTER INSERT ON doc_subscriptions BEGIN DELETE FROM sessions WHERE user_id=NEW.user_id; END","CREATE TRIGGER revoke AFTER INSERT ON reactions BEGIN DELETE FROM space_members WHERE user_id=NEW.user_id; END"]{let s=state().await;sqlx::query(trigger).execute(&s.db).await.unwrap();let result=if trigger.contains("reactions"){toggle_reaction(State(s.clone()),auth("editor-a"),path(),Json(ToggleReactionRequest{emoji:"👍".into()})).await.err()}else{create_comment(State(s.clone()),auth("editor-a"),path(),body(None)).await.err()};assert!(result.is_some());assert_eq!(counts(&s).await,(0,0,0));let active:i64=sqlx::query_scalar("SELECT is_active FROM users WHERE id='editor-a'").fetch_one(&s.db).await.unwrap();assert_eq!(active,1);}
    }
    #[tokio::test]
    async fn comment_existing_mutations_roll_back_late_actor_revocation() {
        for kind in ["update", "resolve", "delete"] {
            let s = state().await;
            seed_comment(&s, "comment", "target-a", "editor-a", None).await;
            let trigger=match kind{
                "update"=>"CREATE TRIGGER revoke AFTER UPDATE ON comments WHEN NEW.content<>OLD.content BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END",
                "resolve"=>"CREATE TRIGGER revoke AFTER UPDATE ON comments WHEN NEW.resolved<>OLD.resolved BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END",
                _=>"CREATE TRIGGER revoke AFTER DELETE ON comments BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END",
            };
            sqlx::query(trigger).execute(&s.db).await.unwrap();
            let denied = match kind {
                "update" => update_comment(
                    State(s.clone()),
                    auth("editor-a"),
                    comment_path("comment"),
                    Json(UpdateCommentRequest {
                        content: "Edited".into(),
                    }),
                )
                .await
                .is_err(),
                "resolve" => {
                    resolve_comment(State(s.clone()), auth("editor-a"), comment_path("comment"))
                        .await
                        .is_err()
                }
                _ => delete_comment(State(s.clone()), auth("editor-a"), comment_path("comment"))
                    .await
                    .is_err(),
            };
            assert!(denied);
            let row:(String,i64,i64)=sqlx::query_as("SELECT content,resolved,(SELECT is_active FROM users WHERE id='editor-a') FROM comments WHERE id='comment'").fetch_one(&s.db).await.unwrap();
            assert_eq!(row, ("Original".into(), 0, 1));
        }
    }
    #[tokio::test]
    async fn comment_delete_rejects_foreign_descendant_cascade() {
        let s = state().await;
        seed_comment(&s, "parent", "target-a", "editor-a", None).await;
        seed_comment(&s, "foreign-child", "target-b", "owner-a", Some("parent")).await;
        assert!(matches!(
            delete_comment(State(s.clone()), auth("editor-a"), comment_path("parent")).await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(counts(&s).await.0, 2);
    }
    #[tokio::test]
    async fn comment_rechecks_authority_after_transaction_wait() {
        let s = state().await;
        let mut blocker = s.db.begin().await.unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id='editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copied = s.clone();
        let mut pending = tokio::spawn(async move {
            create_comment(State(copied), auth("editor-a"), path(), body(None))
                .await
                .map(|r| r.into_response().status())
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut pending)
                .await
                .is_err()
        );
        blocker.commit().await.unwrap();
        assert!(pending.await.unwrap().is_err());
        assert_eq!(counts(&s).await, (0, 0, 0));
    }
    #[tokio::test]
    async fn comment_notifications_omit_unavailable_recipients_and_private_ancestor_titles() {
        let s = state().await;
        private_draft(&s, "editor-a").await;
        for user in ["owner-a", "editor-a", "viewer-a"] {
            sqlx::query("INSERT INTO doc_subscriptions(document_id,user_id) VALUES('target-a',$1)")
                .bind(user)
                .execute(&s.db)
                .await
                .unwrap();
        }
        seed_comment(&s, "parent", "target-a", "owner-a", None).await;
        assert!(create_comment(
            State(s.clone()),
            auth("editor-a"),
            path(),
            body(Some("parent"))
        )
        .await
        .is_ok());
        let sent: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications")
            .fetch_one(&s.db)
            .await
            .unwrap();
        assert_eq!(sent, 0);
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('private','viewer-a','viewer','editor-a')").execute(&s.db).await.unwrap();
        assert!(
            create_comment(State(s.clone()), auth("editor-a"), path(), body(None))
                .await
                .is_ok()
        );
        let recipients: Vec<String> = sqlx::query_scalar("SELECT user_id FROM notifications")
            .fetch_all(&s.db)
            .await
            .unwrap();
        assert_eq!(recipients, vec!["viewer-a"]);
        sqlx::query("UPDATE documents SET published_at='published',parent_id='target-b' WHERE id='target-a'").execute(&s.db).await.unwrap();
        sqlx::query(
            "UPDATE documents SET space_id='space-a',published_at=NULL WHERE id='target-b'",
        )
        .execute(&s.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('ancestor-private','target-b','space-a','editor-a','Ancestor secret','Body','base')").execute(&s.db).await.unwrap();
        notify_comment_recipients(
            &s,
            "space-a",
            "target-a",
            None,
            "editor-a",
            "hidden ancestor",
        )
        .await
        .unwrap();
        let ancestor_count: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications")
            .fetch_one(&s.db)
            .await
            .unwrap();
        assert_eq!(ancestor_count, 1);
        sqlx::query("UPDATE documents SET archived_at='archive' WHERE id='target-a'")
            .execute(&s.db)
            .await
            .unwrap();
        notify_comment_recipients(&s, "space-a", "target-a", None, "editor-a", "later")
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications")
            .fetch_one(&s.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }
}

#[cfg(test)]
mod current_comment_read_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    async fn wal_state() -> AppState {
        let mut state = super::tests::state().await;
        let file = std::env::temp_dir().join(format!("comment-current-{}.sqlite", Uuid::new_v4()));
        let db = sqlx::any::AnyPoolOptions::new()
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
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for (actor, role) in [
            ("owner-a", "owner"),
            ("editor-a", "editor"),
            ("viewer-a", "viewer"),
        ] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')")
                .bind(actor)
                .bind(format!("{actor}@comments.test"))
                .execute(&db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'hash','2999-01-01T00:00:00Z')").bind(format!("comment-{actor}")).bind(actor).execute(&db).await.unwrap();
            if actor == "owner-a" {
                sqlx::query("INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','Space','comment-space','owner-a')").execute(&db).await.unwrap();
            }
            sqlx::query(
                "INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$1,$2)",
            )
            .bind(actor)
            .bind(role)
            .execute(&db)
            .await
            .unwrap();
        }
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','CANONICAL SECRET','','owner-a','owner-a','published')").execute(&db).await.unwrap();
        state.db = db;
        state
    }
    async fn auth(state: &AppState, user: &str) -> AuthUser {
        let token = crate::auth::jwt::create_token_pair_for_session(
            user,
            "comments@test",
            "member",
            &state.jwt_secret,
            Some(&format!("comment-{user}")),
            30,
        )
        .unwrap()
        .access_token;
        let request = axum::http::Request::builder()
            .method("GET")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        <AuthUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(
            &mut parts, state,
        )
        .await
        .unwrap()
    }
    fn path() -> Path<(String, String)> {
        Path(("space-a".into(), "target-a".into()))
    }
    fn change_hook(
        state: &AppState,
        query: &str,
        reached: Arc<AtomicBool>,
    ) -> super::super::documents::NotificationTitleReadHook {
        let db = state.db.clone();
        let query = query.to_string();
        Box::new(move || {
            Box::pin(async move {
                reached.store(true, Ordering::SeqCst);
                for statement in query.split("; ") {
                    sqlx::query(statement).execute(&db).await.unwrap();
                }
            })
        })
    }
    #[tokio::test]
    async fn comments_current_read_wal_revocation_after_load_hides_comments_and_reactions() {
        for reactions in [false, true] {
            for change in [
            "DELETE FROM sessions WHERE id='comment-viewer-a'",
            "UPDATE users SET is_active=0 WHERE id='viewer-a'",
            "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')",
            "INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('target-a','space-a','restricted','revoked','owner-a')",
            "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('private-parent','space-a','PARENT SECRET','','owner-a','owner-a'); UPDATE documents SET parent_id='private-parent' WHERE id='target-a'",
        ] {
            let state=wal_state().await;super::tests::seed_comment(&state,"secret-comment","target-a","owner-a",None).await;
            sqlx::query("INSERT INTO reactions(id,document_id,user_id,emoji) VALUES('reaction','target-a','owner-a','yes')").execute(&state.db).await.unwrap();
            let initial=if reactions {list_reactions(State(state.clone()),auth(&state,"viewer-a").await,path()).await.unwrap().into_response()} else {list_comments(State(state.clone()),auth(&state,"viewer-a").await,path()).await.unwrap().into_response()};
            assert_eq!(initial.status(),StatusCode::OK);
            let reached=Arc::new(AtomicBool::new(false));let hook=change_hook(&state,change,reached.clone());
            let denied=if reactions {list_reactions_current(State(state.clone()),auth(&state,"viewer-a").await,path(),Some(hook)).await.err()} else {list_comments_current(State(state.clone()),auth(&state,"viewer-a").await,path(),Some(hook)).await.err()};
            assert!(denied.is_some());assert!(reached.load(Ordering::SeqCst));
            let retained:i64=sqlx::query_scalar("SELECT COUNT(*) FROM comments WHERE id='secret-comment'").fetch_one(&state.db).await.unwrap();assert_eq!(retained,1);
        }
        }
    }
    #[tokio::test]
    async fn comments_current_read_wal_personal_key_scope_revocation_after_load_denies() {
        for reactions in [false, true] {
            let state = wal_state().await;
            let raw = "comment-read-key";
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('comment-key','viewer-a','Read',$1,'prefix','[\"read\"]')").bind(crate::auth::api_keys::hash_api_key(raw)).execute(&state.db).await.unwrap();
            let request = axum::http::Request::builder()
                .method("GET")
                .header("X-API-Key", raw)
                .body(())
                .unwrap();
            let (mut parts, _) = request.into_parts();
            let actual =
                <AuthUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(
                    &mut parts, &state,
                )
                .await
                .unwrap();
            let reached = Arc::new(AtomicBool::new(false));
            let hook = change_hook(
                &state,
                "UPDATE api_keys SET scopes='[\"write\"]' WHERE id='comment-key'",
                reached.clone(),
            );
            let denied = if reactions {
                list_reactions_current(State(state.clone()), actual, path(), Some(hook))
                    .await
                    .err()
            } else {
                list_comments_current(State(state.clone()), actual, path(), Some(hook))
                    .await
                    .err()
            };
            assert!(denied.is_some());
            assert!(reached.load(Ordering::SeqCst));
        }
    }
    async fn private(state: &AppState) {
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at,updated_at) VALUES('selected','target-a','space-a','owner-a','SELECTED READABLE','old','2099'),('other','target-a','space-a','owner-a','OTHER READABLE','old','2001')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('selected','viewer-a','viewer','owner-a'),('other','viewer-a','viewer','owner-a')").execute(&state.db).await.unwrap();
    }
    #[tokio::test]
    async fn notification_current_title_selects_owned_or_exact_shared_draft_without_canonical_secret(
    ) {
        let state = wal_state().await;
        private(&state).await;
        for user in ["owner-a", "viewer-a"] {
            let mut conn = state.db.acquire().await.unwrap();
            let title = super::super::documents::readable_notification_page_title(
                &mut conn, "space-a", "target-a", user,
            )
            .await
            .unwrap()
            .unwrap()
            .1;
            assert_eq!(title, "SELECTED READABLE");
            assert!(!title.contains("CANONICAL"));
        }
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at,updated_at) VALUES('own-viewer','target-a','space-a','viewer-a','OWN READABLE','old','1900')").execute(&state.db).await.unwrap();
        {
            let mut conn = state.db.acquire().await.unwrap();
            assert_eq!(
                super::super::documents::readable_notification_page_title(
                    &mut conn, "space-a", "target-a", "viewer-a"
                )
                .await
                .unwrap()
                .unwrap()
                .1,
                "OWN READABLE"
            );
        }
        sqlx::query("DELETE FROM document_drafts WHERE id='own-viewer'")
            .execute(&state.db)
            .await
            .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert!(super::super::documents::readable_notification_page_title(
            &mut conn, "space-a", "target-a", "editor-a"
        )
        .await
        .unwrap()
        .is_none());
        drop(conn);
        sqlx::query(
            "INSERT INTO doc_subscriptions(document_id,user_id) VALUES('target-a','viewer-a')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        notify_comment_recipients(
            &state,
            "space-a",
            "target-a",
            None,
            "owner-a",
            "Comment text",
        )
        .await
        .unwrap();
        let titles: Vec<String> =
            sqlx::query_scalar("SELECT title FROM notifications WHERE user_id='viewer-a'")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(titles.len(), 1);
        assert!(titles[0].contains("SELECTED READABLE"));
        assert!(!titles[0].contains("CANONICAL"));
    }
    #[tokio::test]
    async fn notification_current_title_wal_selected_share_revoked_with_other_readable_draft_returns_none(
    ) {
        let state = wal_state().await;
        private(&state).await;
        let reached = Arc::new(AtomicBool::new(false));
        let hook = change_hook(
            &state,
            "DELETE FROM document_draft_access WHERE draft_id='selected' AND user_id='viewer-a'",
            reached.clone(),
        );
        let mut conn = state.db.acquire().await.unwrap();
        assert!(
            super::super::documents::readable_notification_page_title_inner(
                &mut conn,
                "space-a",
                "target-a",
                "viewer-a",
                Some(hook),
                None
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(reached.load(Ordering::SeqCst));
        let principal = super::super::access::Principal::User {
            user_id: "viewer-a",
            session_id: None,
            token_expires_at: None,
        };
        assert!(super::super::access::resolve_page_access(
            &mut conn,
            principal,
            "space-a",
            "target-a",
            super::super::access::PageOperation::View,
            super::super::access::Purpose::Routine
        )
        .await
        .is_ok());
    }
    #[tokio::test]
    async fn notification_current_title_wal_source_publication_and_account_changes_fail_closed() {
        for private_page in [false, true] {
            for change in [
            "UPDATE users SET is_active=0 WHERE id='viewer-a'",
            "UPDATE documents SET title='changed',updated_at='changed' WHERE id='target-a'",
            "UPDATE documents SET published_at=CASE WHEN published_at IS NULL THEN 'new' ELSE NULL END WHERE id='target-a'",
            "UPDATE document_drafts SET title='changed',updated_at='changed' WHERE id='selected'",
        ] {
            if !private_page && change.starts_with("UPDATE document_drafts") {continue;}
            let state=wal_state().await;if private_page {private(&state).await;} else if change.starts_with("UPDATE documents SET published_at") {
                private(&state).await;sqlx::query("UPDATE documents SET published_at='published' WHERE id='target-a'").execute(&state.db).await.unwrap();
            }let reached=Arc::new(AtomicBool::new(false));
            let hook=change_hook(&state,change,reached.clone());let mut conn=state.db.acquire().await.unwrap();
            assert!(super::super::documents::readable_notification_page_title_inner(&mut conn,"space-a","target-a","viewer-a",Some(hook),None).await.unwrap().is_none());assert!(reached.load(Ordering::SeqCst));
        }
        }
    }
    #[tokio::test]
    async fn notification_current_title_last_source_wal_account_and_exact_share_reproof() {
        for private_page in [false, true] {
            for change in [
            "UPDATE users SET is_active=0 WHERE id='viewer-a'",
            "UPDATE documents SET title='LATE CHANGED',updated_at='late' WHERE id='target-a'",
            "UPDATE documents SET published_at=CASE WHEN published_at IS NULL THEN 'new' ELSE NULL END WHERE id='target-a'",
            "UPDATE document_drafts SET title='LATE CHANGED',updated_at='late' WHERE id='selected'",
        ] {
            if !private_page && change.starts_with("UPDATE document_drafts") {continue;}
            let state=wal_state().await;if private_page {private(&state).await;}else if change.starts_with("UPDATE documents SET published_at") {private(&state).await;sqlx::query("UPDATE documents SET published_at='published' WHERE id='target-a'").execute(&state.db).await.unwrap();}
            let reached=Arc::new(AtomicBool::new(false));let hook=change_hook(&state,change,reached.clone());
            let mut conn=state.db.acquire().await.unwrap();assert!(super::super::documents::readable_notification_page_title_inner(&mut conn,"space-a","target-a","viewer-a",None,Some(hook)).await.unwrap().is_none());assert!(reached.load(Ordering::SeqCst));
        }
        }
        let state = wal_state().await;
        private(&state).await;
        let reached = Arc::new(AtomicBool::new(false));
        let hook = change_hook(
            &state,
            "DELETE FROM document_draft_access WHERE draft_id='selected' AND user_id='viewer-a'",
            reached.clone(),
        );
        let mut conn = state.db.acquire().await.unwrap();
        assert!(
            super::super::documents::readable_notification_page_title_inner(
                &mut conn,
                "space-a",
                "target-a",
                "viewer-a",
                None,
                Some(hook)
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(reached.load(Ordering::SeqCst));
        let principal = super::super::access::Principal::User {
            user_id: "viewer-a",
            session_id: None,
            token_expires_at: None,
        };
        assert!(super::super::access::resolve_page_access(
            &mut conn,
            principal,
            "space-a",
            "target-a",
            super::super::access::PageOperation::View,
            super::super::access::Purpose::Routine
        )
        .await
        .is_ok());
    }
}

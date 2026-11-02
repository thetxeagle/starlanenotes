use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use lettre::{
    message::Mailbox, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(list_notifications))
        .route(
            "/notifications/preferences",
            get(get_notification_preferences).put(update_notification_preferences),
        )
        .route(
            "/notifications/read-all",
            axum::routing::post(mark_all_read),
        )
        .route(
            "/notifications/system",
            axum::routing::post(create_system_notification),
        )
        .route(
            "/notifications/{notification_id}/read",
            axum::routing::post(mark_read),
        )
}

const NOTIFICATION_TYPES: &[(&str, &str)] = &[
    ("workflow_approval", "Workflow approvals"),
    ("workflow_handoff", "Workflow handoffs"),
    ("workflow_delegate", "Workflow delegations"),
    ("workflow_escalate", "Workflow escalations"),
    ("mention", "Mentions"),
    ("comment", "Comments"),
    ("comment_reply", "Comment replies"),
    ("doc_shared", "Shared documents"),
    ("space_invite", "Space invites"),
    ("doc_subscription", "Document subscriptions"),
    ("doc_locked", "Document locks"),
    ("doc_unlocked", "Document unlocks"),
    ("system", "System activity"),
];

const NOTIFICATION_CHANNELS: &[&str] = &["both", "in_app", "email", "none"];

#[derive(Debug, PartialEq, Eq)]
struct NotificationDeliveryPlan {
    in_app: bool,
    email: bool,
}

#[derive(Serialize)]
pub struct NotificationResponse {
    pub id: String,
    pub notification_type: String,
    pub title: String,
    pub body: Option<String>,
    pub source_user: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub read: bool,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct NotificationPreferenceResponse {
    pub notification_type: String,
    pub label: String,
    pub channel: String,
}

#[derive(Deserialize)]
struct NotificationPreferenceUpdate {
    notification_type: String,
    channel: String,
}

#[derive(Deserialize)]
struct UpdateNotificationPreferencesRequest {
    preferences: Vec<NotificationPreferenceUpdate>,
}

#[derive(Deserialize)]
struct CreateSystemNotificationRequest {
    title: String,
    body: Option<String>,
}

#[derive(Deserialize, Default)]
struct NotificationListQuery {
    limit: Option<i64>,
    before: Option<String>,
    unread_only: Option<bool>,
}

fn notification_cursor_parts(
    cursor: Option<&str>,
) -> Result<(Option<String>, Option<String>), AppError> {
    let Some(cursor) = cursor else {
        return Ok((None, None));
    };

    let Some((created_at, id)) = cursor.rsplit_once('|') else {
        return Err(AppError::BadRequest("Invalid notification cursor".into()));
    };
    if created_at.is_empty() || id.is_empty() {
        return Err(AppError::BadRequest("Invalid notification cursor".into()));
    }

    Ok((Some(created_at.to_string()), Some(id.to_string())))
}

fn validate_notification_preference_updates(
    preferences: &[NotificationPreferenceUpdate],
) -> Result<(), AppError> {
    for preference in preferences {
        if !NOTIFICATION_TYPES
            .iter()
            .any(|(notification_type, _)| *notification_type == preference.notification_type)
        {
            return Err(AppError::BadRequest("Unknown notification type".into()));
        }
        if !NOTIFICATION_CHANNELS.contains(&preference.channel.as_str()) {
            return Err(AppError::BadRequest("Unknown notification channel".into()));
        }
    }

    Ok(())
}

fn notification_delivery_plan(channel: Option<&str>) -> NotificationDeliveryPlan {
    match channel {
        Some("none") => NotificationDeliveryPlan {
            in_app: false,
            email: false,
        },
        Some("in_app") => NotificationDeliveryPlan {
            in_app: true,
            email: false,
        },
        Some("email") => NotificationDeliveryPlan {
            in_app: false,
            email: true,
        },
        Some("both") | None => NotificationDeliveryPlan {
            in_app: true,
            email: true,
        },
        Some(_) => NotificationDeliveryPlan {
            in_app: true,
            email: true,
        },
    }
}

fn default_notification_channel(notification_type: &str) -> &'static str {
    // App-local confirmation messages belong in the inbox/banner, never email.
    if notification_type == "system" {
        "in_app"
    } else {
        "both"
    }
}

/// Contextual notification delivery is unsupported for guests. Preserve records
/// and preferences; classification never rewrites previously downloaded content.
async fn notification_member(conn: &mut sqlx::AnyConnection, user: &str) -> Result<bool, AppError> {
    let kind: Option<String> = sqlx::query_scalar("SELECT account_kind FROM users WHERE id=$1 AND is_active=1")
        .bind(user).fetch_optional(conn).await.map_err(AppError::internal)?;
    Ok(kind.as_deref() == Some("member"))
}

async fn page_notification_access(
    conn: &mut sqlx::AnyConnection,
    principal: crate::spaces::access::Principal<'_>,
    target_type: Option<&str>,
    target_id: Option<&str>,
    operation: crate::spaces::access::PageOperation,
) -> Result<bool, AppError> {
    if let crate::spaces::access::Principal::User { user_id, .. } | crate::spaces::access::Principal::UserKey { user_id, .. } = principal {
        if !notification_member(conn, user_id).await? { return Ok(false); }
    }
    if target_type == Some("draft_discussion") {
        let Some(id) = target_id.filter(|id| !id.is_empty()) else {
            return Ok(false);
        };
        let context: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT space_id,document_id,draft_id,state FROM draft_discussions WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
        let Some((space, document, draft, state)) = context else {
            return Ok(false);
        };
        if state != "active" {
            return Ok(false);
        }
        return Ok(crate::spaces::access::resolve_draft_comment_access(
            conn,
            principal,
            &space,
            &document,
            &draft,
            crate::spaces::access::Purpose::Routine,
        )
        .await
        .is_ok());
    }
    if target_type != Some("document") {
        return Ok(true);
    }
    let Some(id) = target_id.filter(|id| !id.is_empty()) else {
        return Ok(false);
    };
    let Some(space) = sqlx::query_scalar::<_, String>(
        "SELECT space_id FROM documents WHERE id=$1 AND deleted_at IS NULL AND archived_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?
    else {
        return Ok(false);
    };
    Ok(crate::spaces::access::resolve_page_access(
        conn,
        principal,
        &space,
        id,
        operation,
        crate::spaces::access::Purpose::Routine,
    )
    .await
    .is_ok())
}
async fn page_notification_visible(
    conn: &mut sqlx::AnyConnection,
    principal: crate::spaces::access::Principal<'_>,
    target_type: Option<&str>,
    target_id: Option<&str>,
) -> Result<bool, AppError> {
    page_notification_access(
        conn,
        principal,
        target_type,
        target_id,
        crate::spaces::access::PageOperation::View,
    )
    .await
}
async fn page_notification_acknowledgeable(
    conn: &mut sqlx::AnyConnection,
    principal: crate::spaces::access::Principal<'_>,
    target_type: Option<&str>,
    target_id: Option<&str>,
) -> Result<bool, AppError> {
    page_notification_access(
        conn,
        principal,
        target_type,
        target_id,
        crate::spaces::access::PageOperation::AcknowledgeNotification,
    )
    .await
}
async fn current_page_delivery_visible(
    db: &sqlx::AnyPool,
    user_id: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
) -> bool {
    let Ok(mut conn) = db.acquire().await else {
        return false;
    };
    if !notification_member(&mut conn, user_id).await.unwrap_or(false) { return false; }
    if !matches!(target_type, Some("document" | "draft_discussion")) { return true; }
    page_notification_visible(
        &mut conn,
        crate::spaces::access::Principal::User {
            user_id,
            session_id: None,
            token_expires_at: None,
        },
        target_type,
        target_id,
    )
    .await
    .unwrap_or(false)
}

#[cfg(test)]
thread_local! {
    static BEFORE_INBOX_FINAL_CHECK: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>>> = std::cell::RefCell::new(None);
}

async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<NotificationListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let (mut before_created_at, mut before_id) =
        notification_cursor_parts(query.before.as_deref())?;
    let unread_only = i32::from(query.unread_only.unwrap_or(false));
    crate::spaces::access::principal_from_auth(&auth)?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let mut result = Vec::new();
    loop {
        let notifications: Vec<(
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            i32,
            String,
        )> = sqlx::query_as(
            "SELECT n.id, n.type, n.title, n.body, u.display_name, n.target_type, \
             n.target_id, n.read, n.created_at \
             FROM notifications n \
             LEFT JOIN users u ON n.source_user_id = u.id \
             WHERE n.user_id = $1 \
               AND ($2 = 0 OR n.read = 0) \
               AND ($3 IS NULL OR n.created_at < $3 OR (n.created_at = $3 AND n.id < $4)) \
             ORDER BY n.created_at DESC, n.id DESC LIMIT $5",
        )
        .bind(&auth.claims.sub)
        .bind(unread_only)
        .bind(&before_created_at)
        .bind(&before_id)
        .bind(100_i64)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?;

        if notifications.is_empty() {
            break;
        }
        let exhausted = notifications.len() < 100;
        for (id, ntype, title, body, source, ttype, tid, read, created) in notifications {
            before_created_at = Some(created.clone());
            before_id = Some(id.clone());
            let principal = crate::spaces::access::principal_from_auth(&auth)?;
            if !page_notification_visible(&mut conn, principal, ttype.as_deref(), tid.as_deref())
                .await?
            {
                continue;
            }
            result.push(NotificationResponse {
                id,
                notification_type: ntype,
                title,
                body,
                source_user: source,
                target_type: ttype,
                target_id: tid,
                read: read != 0,
                created_at: created,
            });
            if result.len() >= limit as usize {
                break;
            }
        }
        if exhausted || result.len() >= limit as usize {
            break;
        }
    }
    #[cfg(test)]
    {
        let hook = BEFORE_INBOX_FINAL_CHECK.with(|slot| slot.borrow_mut().take());
        if let Some(hook) = hook { hook.await; }
    }
    // Recheck page visibility after the final database waits. Non-page entries
    // retain their existing authenticated middleware contract.
    let mut visible = Vec::new();
    for notification in result {
        let principal = crate::spaces::access::principal_from_auth(&auth)?;
        if page_notification_visible(
            &mut conn,
            principal,
            notification.target_type.as_deref(),
            notification.target_id.as_deref(),
        )
        .await?
        {
            visible.push(notification);
        }
    }
    // A later row's checks may wait after an earlier row was accepted.
    if !notification_member(&mut conn, &auth.claims.sub).await? { visible.clear(); }
    Ok(Json(visible))
}

#[cfg(test)]
mod tests {
    use super::{
        notification_cursor_parts, notification_delivery_plan,
        validate_notification_preference_updates, NotificationDeliveryPlan,
        NotificationPreferenceUpdate,
    };

    #[test]
    fn notification_cursor_requires_timestamp_and_id() {
        assert_eq!(
            notification_cursor_parts(Some("2026-07-12T20:00:00Z|notification-1")).unwrap(),
            (
                Some("2026-07-12T20:00:00Z".to_string()),
                Some("notification-1".to_string())
            )
        );
        assert!(notification_cursor_parts(Some("not-a-cursor")).is_err());
        assert!(notification_cursor_parts(Some("timestamp|")).is_err());
    }

    #[test]
    fn notification_preference_updates_require_known_types_and_channels() {
        assert!(validate_notification_preference_updates(&[
            NotificationPreferenceUpdate {
                notification_type: "comment".to_string(),
                channel: "both".to_string(),
            },
            NotificationPreferenceUpdate {
                notification_type: "workflow_handoff".to_string(),
                channel: "in_app".to_string(),
            },
            NotificationPreferenceUpdate {
                notification_type: "doc_shared".to_string(),
                channel: "email".to_string(),
            },
            NotificationPreferenceUpdate {
                notification_type: "doc_locked".to_string(),
                channel: "none".to_string(),
            },
        ])
        .is_ok());

        assert!(
            validate_notification_preference_updates(&[NotificationPreferenceUpdate {
                notification_type: "unknown".to_string(),
                channel: "both".to_string(),
            }])
            .is_err()
        );

        assert!(
            validate_notification_preference_updates(&[NotificationPreferenceUpdate {
                notification_type: "comment".to_string(),
                channel: "sms".to_string(),
            }])
            .is_err()
        );
    }

    #[test]
    fn notification_delivery_plan_maps_channels_to_delivery_targets() {
        assert_eq!(
            notification_delivery_plan(None),
            NotificationDeliveryPlan {
                in_app: true,
                email: true
            }
        );
        assert_eq!(
            notification_delivery_plan(Some("both")),
            NotificationDeliveryPlan {
                in_app: true,
                email: true
            }
        );
        assert_eq!(
            notification_delivery_plan(Some("in_app")),
            NotificationDeliveryPlan {
                in_app: true,
                email: false
            }
        );
        assert_eq!(
            notification_delivery_plan(Some("email")),
            NotificationDeliveryPlan {
                in_app: false,
                email: true
            }
        );
        assert_eq!(
            notification_delivery_plan(Some("none")),
            NotificationDeliveryPlan {
                in_app: false,
                email: false
            }
        );
        assert_eq!(
            notification_delivery_plan(Some("legacy")),
            NotificationDeliveryPlan {
                in_app: true,
                email: true
            }
        );
    }
    async fn notification_fixture() -> crate::AppState {
        let state = crate::spaces::properties::tests::property_test_state().await;
        for user in ["owner-a", "editor-a", "viewer-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'hash','2999-01-01T00:00:00Z')").bind(format!("notification-{user}")).bind(user).execute(&state.db).await.unwrap();
        }
        state
    }
    fn notification_auth(user: &str) -> crate::auth::middleware::AuthUser {
        let mut a = crate::spaces::properties::tests::test_auth(user);
        a.claims.sid = format!("notification-{user}");
        a
    }
    async fn seed_notification(
        state: &crate::AppState,
        id: &str,
        kind: Option<&str>,
        target: Option<&str>,
        created: &str,
    ) {
        sqlx::query("INSERT INTO notifications(id,user_id,type,title,body,target_type,target_id,created_at) VALUES($1,'editor-a','workflow_approval','Secret title','Secret body',$2,$3,$4)").bind(id).bind(kind).bind(target).bind(created).execute(&state.db).await.unwrap();
    }
    async fn inbox(
        state: &crate::AppState,
        query: super::NotificationListQuery,
    ) -> serde_json::Value {
        use axum::response::IntoResponse;
        let response = super::list_notifications(
            axum::extract::State(state.clone()),
            notification_auth("editor-a"),
            axum::extract::Query(query),
        )
        .await
        .unwrap()
        .into_response();
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn guest_contextual_inbox_ack_and_delivery_retain_stored_space_and_page_rows() {
        let s=notification_fixture().await;
        seed_notification(&s,"space",Some("space"),Some("space-a"),"2027").await;
        seed_notification(&s,"page",Some("document"),Some("target-a"),"2026").await;
        assert_eq!(inbox(&s,Default::default()).await.as_array().unwrap().len(),2);
        sqlx::query("UPDATE users SET account_kind='guest' WHERE id='editor-a'").execute(&s.db).await.unwrap();
        assert_eq!(inbox(&s,Default::default()).await,serde_json::json!([]));
        assert!(super::mark_read(axum::extract::State(s.clone()),notification_auth("editor-a"),axum::extract::Path("space".into())).await.is_err());
        super::mark_all_read(axum::extract::State(s.clone()),notification_auth("editor-a")).await.unwrap();
        super::notify(&s.db,"editor-a","space_invite","Hidden new space",None,None,Some("space"),Some("space-a")).await;
        let rows:Vec<(String,i64)>=sqlx::query_as("SELECT id,read FROM notifications WHERE user_id='editor-a' ORDER BY id").fetch_all(&s.db).await.unwrap();
        assert_eq!(rows,vec![("page".into(),0),("space".into(),0)]);
    }
    #[tokio::test]
    async fn guest_contextual_inbox_late_classification_omits_old_space_title() {
        let mut s=notification_fixture().await;
        let path=std::env::temp_dir().join(format!("guest-notification-wal-{}.db",uuid::Uuid::new_v4()));
        s.db=sqlx::any::AnyPoolOptions::new().max_connections(2).after_connect(|conn,_|Box::pin(async move {
            sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *conn).await?;
            sqlx::query("PRAGMA busy_timeout=5000").execute(conn).await?;Ok(())
        })).connect(&format!("sqlite://{}?mode=rwc",path.display())).await.unwrap();
        sqlx::migrate!("./migrations").run(&s.db).await.unwrap();
        for sql in [
            "INSERT INTO users(id,email,display_name,role) VALUES('editor-a','guest-wal@example.test','Editor','member')",
            "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('notification-editor-a','editor-a','hash','2999-01-01T00:00:00Z')",
            "INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','Space','guest-notification-wal','editor-a')",
            "INSERT INTO space_members(id,space_id,user_id,role) VALUES('editor','space-a','editor-a','editor')",
        ] {sqlx::query(sql).execute(&s.db).await.unwrap();}
        seed_notification(&s,"space",Some("space"),Some("space-a"),"2027").await;
        let db=s.db.clone();super::BEFORE_INBOX_FINAL_CHECK.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {
            sqlx::query("UPDATE users SET account_kind='guest' WHERE id='editor-a'").execute(&db).await.unwrap();
        })));
        assert_eq!(inbox(&s,Default::default()).await,serde_json::json!([]));
        let unread:i64=sqlx::query_scalar("SELECT read FROM notifications WHERE id='space'").fetch_one(&s.db).await.unwrap();assert_eq!(unread,0);
        let kind:String=sqlx::query_scalar("SELECT account_kind FROM users WHERE id='editor-a'").fetch_one(&s.db).await.unwrap();assert_eq!(kind,"guest");
        s.db.close().await;std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn guest_contextual_email_late_classification_after_configuration_does_not_send() {
        let s=notification_fixture().await;
        sqlx::query("INSERT INTO notification_prefs(id,user_id,notification_type,channel) VALUES('guest-email','editor-a','space_invite','email')").execute(&s.db).await.unwrap();
        for (key,value) in [("smtp_host","localhost"),("smtp_port","1"),("smtp_from","sender@example.test")] {
            sqlx::query("INSERT INTO system_config(key,value) VALUES($1,$2) ON CONFLICT(key) DO UPDATE SET value=excluded.value").bind(key).bind(value).execute(&s.db).await.unwrap();
        }
        assert!(super::current_page_delivery_visible(&s.db,"editor-a",Some("space"),Some("space-a")).await);
        let db=s.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let signal=reached.clone();
        super::BEFORE_EMAIL_DELIVERY_CHECK.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {
            sqlx::query("UPDATE users SET account_kind='guest' WHERE id='editor-a'").execute(&db).await.unwrap();signal.store(true,std::sync::atomic::Ordering::SeqCst);
        })));
        super::send_email_notification(&s.db,"editor-a","space_invite","Hidden space",None,Some("space"),Some("space-a")).await.unwrap();
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!super::current_page_delivery_visible(&s.db,"editor-a",Some("space"),Some("space-a")).await);
    }

    #[tokio::test]
    async fn stored_page_notifications_hide_revoked_private_lifecycle_and_foreign_targets() {
        let s = notification_fixture().await;
        seed_notification(
            &s,
            "visible",
            Some("document"),
            Some("target-a"),
            "2026-01-02",
        )
        .await;
        seed_notification(
            &s,
            "foreign",
            Some("document"),
            Some("target-b"),
            "2026-01-03",
        )
        .await;
        seed_notification(
            &s,
            "missing",
            Some("document"),
            Some("missing"),
            "2026-01-04",
        )
        .await;
        seed_notification(&s, "system", None, None, "2026-01-01").await;
        assert_eq!(
            inbox(&s, Default::default())
                .await
                .as_array()
                .unwrap()
                .len(),
            2
        );
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('private','target-a','space-a','owner-a','Private','base')").execute(&s.db).await.unwrap();
        let hidden = inbox(&s, Default::default()).await;
        assert_eq!(hidden.as_array().unwrap().len(), 1);
        assert_eq!(hidden[0]["id"], "system");
        let unavailable = super::mark_read(
            axum::extract::State(s.clone()),
            notification_auth("editor-a"),
            axum::extract::Path("visible".into()),
        )
        .await
        .err()
        .unwrap();
        let missing = super::mark_read(
            axum::extract::State(s.clone()),
            notification_auth("editor-a"),
            axum::extract::Path("unknown".into()),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(format!("{unavailable:?}"), format!("{missing:?}"));
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('private','editor-a','viewer','owner-a')").execute(&s.db).await.unwrap();
        assert_eq!(
            inbox(&s, Default::default())
                .await
                .as_array()
                .unwrap()
                .len(),
            2
        );
        for sql in [
            "UPDATE documents SET archived_at='archive' WHERE id='target-a'",
            "UPDATE documents SET archived_at=NULL,deleted_at='trash' WHERE id='target-a'",
            "UPDATE documents SET deleted_at=NULL,parent_id='target-b' WHERE id='target-a'",
        ] {
            sqlx::query(sql).execute(&s.db).await.unwrap();
            assert_eq!(
                inbox(&s, Default::default())
                    .await
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
        }
    }
    #[tokio::test]
    async fn stored_page_inbox_scans_hidden_rows_and_preserves_visible_cursor_and_unread_filter() {
        let s = notification_fixture().await;
        for i in 0..105 {
            seed_notification(
                &s,
                &format!("hidden-{i:03}"),
                Some("document"),
                Some("target-b"),
                "2026-01-03",
            )
            .await;
        }
        seed_notification(&s, "page", Some("document"), Some("target-a"), "2026-01-02").await;
        seed_notification(&s, "system", None, None, "2026-01-01").await;
        let one = inbox(
            &s,
            super::NotificationListQuery {
                limit: Some(1),
                before: None,
                unread_only: Some(true),
            },
        )
        .await;
        assert_eq!(one.as_array().unwrap().len(), 1);
        assert_eq!(one[0]["id"], "page");
        let next = inbox(
            &s,
            super::NotificationListQuery {
                limit: Some(1),
                before: Some("2026-01-02|page".into()),
                unread_only: Some(true),
            },
        )
        .await;
        assert_eq!(next[0]["id"], "system");
        super::mark_all_read(
            axum::extract::State(s.clone()),
            notification_auth("editor-a"),
        )
        .await
        .unwrap();
        assert!(inbox(
            &s,
            super::NotificationListQuery {
                unread_only: Some(true),
                ..Default::default()
            }
        )
        .await
        .as_array()
        .unwrap()
        .is_empty());
        let unread: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM notifications WHERE id LIKE 'hidden-%' AND read=0",
        )
        .fetch_one(&s.db)
        .await
        .unwrap();
        assert_eq!(unread, 105);
    }
    #[tokio::test]
    async fn stored_page_inbox_rechecks_actual_sessions_key_scope_and_actor_after_wait() {
        let s = notification_fixture().await;
        seed_notification(&s, "page", Some("document"), Some("target-a"), "2026").await;
        sqlx::query("DELETE FROM sessions WHERE user_id='editor-a'")
            .execute(&s.db)
            .await
            .unwrap();
        assert!(inbox(&s, Default::default())
            .await
            .as_array()
            .unwrap()
            .is_empty());
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('notification-key','editor-a','Key','hash','prefix','[\"write\"]')").execute(&s.db).await.unwrap();
        let mut key = notification_auth("editor-a");
        key.claims.sid = "api-key:notification-key".into();
        key.api_key_scopes = Some(vec!["admin".into()]);
        use axum::response::IntoResponse;
        let result = super::list_notifications(
            axum::extract::State(s.clone()),
            key,
            axum::extract::Query(Default::default()),
        )
        .await
        .unwrap()
        .into_response();
        let bytes = axum::body::to_bytes(result.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            serde_json::json!([])
        );
        let s = notification_fixture().await;
        seed_notification(&s, "page", Some("document"), Some("target-a"), "2026").await;
        let mut blocker = s.db.begin().await.unwrap();
        sqlx::query("DELETE FROM space_members WHERE user_id='editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copied = s.clone();
        let mut pending = tokio::spawn(async move { inbox(&copied, Default::default()).await });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut pending)
                .await
                .is_err()
        );
        blocker.commit().await.unwrap();
        assert!(pending.await.unwrap().as_array().unwrap().is_empty());
    }
    #[tokio::test]
    async fn inbox_final_check_observes_external_wal_revocation() {
        for change in [
            "DELETE FROM sessions WHERE user_id='editor-a'",
            "UPDATE documents SET published_at=NULL WHERE id='target-a'",
        ] {
            let mut state = notification_fixture().await;
            let path = std::env::temp_dir().join(format!("notification-wal-{}.db", uuid::Uuid::new_v4()));
            state.db = sqlx::any::AnyPoolOptions::new().max_connections(2)
                .after_connect(|conn, _| Box::pin(async move {
                    sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *conn).await?;
                    sqlx::query("PRAGMA busy_timeout=5000").execute(conn).await?;
                    Ok(())
                }))
                .connect(&format!("sqlite://{}?mode=rwc", path.display())).await.unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            for sql in [
                "INSERT INTO users(id,email,display_name,role) VALUES('editor-a','wal@example.test','Editor','member')",
                "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('notification-editor-a','editor-a','hash','2999-01-01T00:00:00Z')",
                "INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','Space','notification-wal','editor-a')",
                "INSERT INTO space_members(id,space_id,user_id,role) VALUES('editor','space-a','editor-a','editor')",
                "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Target','','editor-a','editor-a','published')",
            ] { sqlx::query(sql).execute(&state.db).await.unwrap(); }
            seed_notification(&state, "page", Some("document"), Some("target-a"), "2026").await;
            let writer = state.db.clone();
            super::BEFORE_INBOX_FINAL_CHECK.with(|slot| {
                *slot.borrow_mut() = Some(Box::pin(async move {
                    // A separate pooled connection commits while the reader is held.
                    sqlx::query(change).execute(&writer).await.unwrap();
                }));
            });
            let response = inbox(&state, Default::default()).await;
            assert_eq!(response, serde_json::json!([]), "External change remained hidden: {change}");
            state.db.close().await;
            std::fs::remove_file(path).unwrap();
        }
    }

    #[tokio::test]
    async fn stored_page_read_rolls_back_late_revocation() {
        for all in [false, true] {
            let s = notification_fixture().await;
            seed_notification(&s, "page", Some("document"), Some("target-a"), "2026").await;
            let sessions_before: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM sessions WHERE user_id='editor-a' ORDER BY id",
            ).fetch_all(&s.db).await.unwrap();
            sqlx::query("CREATE TRIGGER revoke AFTER UPDATE ON notifications WHEN NEW.read<>OLD.read BEGIN DELETE FROM sessions WHERE user_id=NEW.user_id; END").execute(&s.db).await.unwrap();
            let denied = if all {
                super::mark_all_read(
                    axum::extract::State(s.clone()),
                    notification_auth("editor-a"),
                )
                .await
                .is_err()
            } else {
                super::mark_read(
                    axum::extract::State(s.clone()),
                    notification_auth("editor-a"),
                    axum::extract::Path("page".into()),
                )
                .await
                .is_err()
            };
            assert!(denied);
            let read: i64 = sqlx::query_scalar("SELECT read FROM notifications WHERE id='page'")
                .fetch_one(&s.db)
                .await
                .unwrap();
            assert_eq!(read, 0);
            let sessions: Vec<String> =
                sqlx::query_scalar("SELECT id FROM sessions WHERE user_id='editor-a' ORDER BY id")
                    .fetch_all(&s.db)
                    .await
                    .unwrap();
            assert_eq!(sessions, sessions_before);
        }
    }
    #[tokio::test]
    async fn page_delivery_never_stores_hidden_titles_for_unshared_or_inactive_users() {
        let s = notification_fixture().await;
        super::notify(
            &s.db,
            "editor-a",
            "workflow_approval",
            "Visible title",
            Some("Visible body"),
            Some("owner-a"),
            Some("document"),
            Some("target-a"),
        )
        .await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&s.db)
            .await
            .unwrap();
        super::notify(
            &s.db,
            "editor-a",
            "workflow_approval",
            "Private title",
            Some("Private body"),
            Some("owner-a"),
            Some("document"),
            Some("target-a"),
        )
        .await;
        sqlx::query("UPDATE documents SET published_at='published' WHERE id='target-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET is_active=0 WHERE id='editor-a'")
            .execute(&s.db)
            .await
            .unwrap();
        super::notify(
            &s.db,
            "editor-a",
            "workflow_approval",
            "Inactive title",
            None,
            Some("owner-a"),
            Some("document"),
            Some("target-a"),
        )
        .await;
        let titles: Vec<String> = sqlx::query_scalar("SELECT title FROM notifications")
            .fetch_all(&s.db)
            .await
            .unwrap();
        assert_eq!(titles, vec!["Visible title"]);
    }
    #[tokio::test]
    async fn draft_discussion_notifications_require_exact_active_participation_for_delivery_inbox_and_ack() {
        let s=notification_fixture().await;
        sqlx::query("INSERT INTO notification_prefs(id,user_id,notification_type,channel) VALUES('draft-mention-pref','editor-a','mention','in_app')").execute(&s.db).await.unwrap();
        for sql in [
            "UPDATE documents SET published_at=NULL WHERE id='target-a'",
            "INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('draft-a','target-a','space-a','owner-a','Private','base')",
            "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('draft-a','editor-a','viewer','owner-a')",
            "INSERT INTO draft_discussions(id,space_id,document_id,draft_id,draft_created_at,draft_owner_id,current_source_digest,state,created_at,updated_at) VALUES('discussion-a','space-a','target-a','draft-a','created','owner-a','digest','active','created','created')",
        ]{sqlx::query(sql).execute(&s.db).await.unwrap();}
        super::notify(&s.db,"editor-a","mention","Private draft mention",Some("Private draft body"),Some("owner-a"),Some("draft_discussion"),Some("discussion-a")).await;
        assert_eq!(inbox(&s,Default::default()).await.as_array().unwrap().len(),1);
        sqlx::query("DELETE FROM document_draft_access WHERE draft_id='draft-a' AND user_id='editor-a'").execute(&s.db).await.unwrap();
        assert_eq!(inbox(&s,Default::default()).await,serde_json::json!([]));
        assert!(super::mark_read(axum::extract::State(s.clone()),notification_auth("editor-a"),axum::extract::Path(sqlx::query_scalar::<_,String>("SELECT id FROM notifications WHERE user_id='editor-a'").fetch_one(&s.db).await.unwrap())).await.is_err());
        super::notify(&s.db,"editor-a","mention","Must not persist",None,Some("owner-a"),Some("draft_discussion"),Some("discussion-a")).await;
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id='editor-a'").fetch_one(&s.db).await.unwrap();
        assert_eq!(count,1);
    }
    #[tokio::test]
    async fn email_delivery_rechecks_page_visibility_after_configuration_waits() {
        let s = notification_fixture().await;
        sqlx::query("INSERT INTO notification_prefs(id,user_id,notification_type,channel) VALUES('email-pref','editor-a','workflow_approval','email')")
            .execute(&s.db).await.unwrap();
        for (key, value) in [
            ("smtp_host", "localhost"),
            ("smtp_port", "1"),
            ("smtp_from", "sender@example.test"),
        ] {
            sqlx::query("INSERT INTO system_config(key,value) VALUES($1,$2) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
                .bind(key).bind(value).execute(&s.db).await.unwrap();
        }
        assert!(
            super::current_page_delivery_visible(
                &s.db,
                "editor-a",
                Some("document"),
                Some("target-a")
            )
            .await
        );
        let db = s.db.clone();
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reached_hook = reached.clone();
        super::BEFORE_EMAIL_DELIVERY_CHECK.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                // Revocation happens after all recipient/config reads, before SMTP.
                sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
                    .execute(&db)
                    .await
                    .unwrap();
                reached_hook.store(true, std::sync::atomic::Ordering::SeqCst);
            }))
        });
        super::send_email_notification(
            &s.db,
            "editor-a",
            "workflow_approval",
            "Secret title",
            Some("Secret body"),
            Some("document"),
            Some("target-a"),
        )
        .await
        .unwrap();
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
        assert!(
            !super::current_page_delivery_visible(
                &s.db,
                "editor-a",
                Some("document"),
                Some("target-a")
            )
            .await
        );
        // localhost:1 cannot deliver: success proves delivery was suppressed.
    }
    #[tokio::test]
    async fn notification_http_key_acknowledgment_preserves_method_scope_and_current_page_view() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let s = notification_fixture().await;
        seed_notification(&s, "page", Some("document"), Some("target-a"), "2026").await;
        let raw = "notification-test-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('http-key','editor-a','HTTP key',$1,'prefix','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key(raw)).execute(&s.db).await.unwrap();
        let request = |method: &str, uri: &str| {
            Request::builder()
                .method(method)
                .uri(uri)
                .header("X-API-Key", raw)
                .body(Body::empty())
                .unwrap()
        };
        let app = super::router().with_state(s.clone());
        assert_eq!(
            app.clone()
                .oneshot(request("GET", "/notifications"))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::FORBIDDEN
        );
        assert_eq!(
            app.clone()
                .oneshot(request("POST", "/notifications/page/read"))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::NO_CONTENT
        );
        sqlx::query("UPDATE notifications SET read=0")
            .execute(&s.db)
            .await
            .unwrap();
        assert_eq!(
            app.clone()
                .oneshot(request("POST", "/notifications/read-all"))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::NO_CONTENT
        );
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='http-key'")
            .execute(&s.db)
            .await
            .unwrap();
        assert_eq!(
            app.clone()
                .oneshot(request("GET", "/notifications"))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(request("POST", "/notifications/page/read"))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::FORBIDDEN
        );
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]' WHERE id='http-key'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&s.db)
            .await
            .unwrap();
        assert_eq!(
            app.clone()
                .oneshot(request("POST", "/notifications/page/read"))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::NOT_FOUND
        );
        sqlx::query("UPDATE documents SET published_at='published' WHERE id='target-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("UPDATE notifications SET read=0")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER scope_revoke AFTER UPDATE ON notifications WHEN NEW.read<>OLD.read BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='http-key'; END").execute(&s.db).await.unwrap();
        assert_eq!(
            app.oneshot(request("POST", "/notifications/page/read"))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::NOT_FOUND
        );
        let read: i64 = sqlx::query_scalar("SELECT read FROM notifications WHERE id='page'")
            .fetch_one(&s.db)
            .await
            .unwrap();
        assert_eq!(read, 0);
        let scope: String = sqlx::query_scalar("SELECT scopes FROM api_keys WHERE id='http-key'")
            .fetch_one(&s.db)
            .await
            .unwrap();
        assert_eq!(scope, "[\"write\"]");
    }
}

async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    crate::spaces::access::principal_from_auth(&auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    let locked = sqlx::query("UPDATE notifications SET read=read WHERE id=$1 AND user_id=$2")
        .bind(&id)
        .bind(&auth.claims.sub)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(AppError::NotFound("Notification not found".into()));
    }
    let target: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT target_type,target_id FROM notifications WHERE id=$1 AND user_id=$2",
    )
    .bind(&id)
    .bind(&auth.claims.sub)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    let Some((kind, target)) = target else {
        return Err(AppError::NotFound("Notification not found".into()));
    };
    if !page_notification_acknowledgeable(
        &mut tx,
        crate::spaces::access::principal_from_auth(&auth)?,
        kind.as_deref(),
        target.as_deref(),
    )
    .await?
    {
        return Err(AppError::NotFound("Notification not found".into()));
    }
    let row = sqlx::query("UPDATE notifications SET read=1,read_at=$1 WHERE id=$2 AND user_id=$3")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&id)
        .bind(&auth.claims.sub)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if row.rows_affected() != 1
        || !page_notification_acknowledgeable(
            &mut tx,
            crate::spaces::access::principal_from_auth(&auth)?,
            kind.as_deref(),
            target.as_deref(),
        )
        .await?
    {
        return Err(AppError::NotFound("Notification not found".into()));
    }
    tx.commit().await.map_err(AppError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn mark_all_read(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    crate::spaces::access::principal_from_auth(&auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    let targets:Vec<(String,Option<String>,Option<String>)>=sqlx::query_as("SELECT id,target_type,target_id FROM notifications WHERE user_id=$1 AND read=0 ORDER BY id").bind(&auth.claims.sub).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    let mut marked = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();
    for (id, _, _) in targets {
        let locked =
            sqlx::query("UPDATE notifications SET read=read WHERE id=$1 AND user_id=$2 AND read=0")
                .bind(&id)
                .bind(&auth.claims.sub)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
        if locked.rows_affected() == 0 {
            continue;
        }
        let (kind, target): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT target_type,target_id FROM notifications WHERE id=$1 AND user_id=$2",
        )
        .bind(&id)
        .bind(&auth.claims.sub)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        if !page_notification_acknowledgeable(
            &mut tx,
            crate::spaces::access::principal_from_auth(&auth)?,
            kind.as_deref(),
            target.as_deref(),
        )
        .await?
        {
            continue;
        }
        let row = sqlx::query(
            "UPDATE notifications SET read=1,read_at=$1 WHERE id=$2 AND user_id=$3 AND read=0",
        )
        .bind(&now)
        .bind(&id)
        .bind(&auth.claims.sub)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        if row.rows_affected() > 1 {
            return Err(AppError::Conflict(
                "Notification read target changed".into(),
            ));
        }
        if row.rows_affected() == 1 {
            marked.push((kind, target));
        }
    }
    for (kind, target) in marked {
        if !page_notification_acknowledgeable(
            &mut tx,
            crate::spaces::access::principal_from_auth(&auth)?,
            kind.as_deref(),
            target.as_deref(),
        )
        .await?
        {
            return Err(AppError::Conflict(
                "Notification access changed; retry".into(),
            ));
        }
    }
    tx.commit().await.map_err(AppError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_notification_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT notification_type, channel FROM notification_prefs WHERE user_id = $1",
    )
    .bind(&auth.claims.sub)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let configured: std::collections::HashMap<String, String> = rows.into_iter().collect();
    let result: Vec<NotificationPreferenceResponse> = NOTIFICATION_TYPES
        .iter()
        .map(
            |(notification_type, label)| NotificationPreferenceResponse {
                notification_type: (*notification_type).to_string(),
                label: (*label).to_string(),
                channel: configured
                    .get(*notification_type)
                    .cloned()
                    .unwrap_or_else(|| default_notification_channel(notification_type).to_string()),
            },
        )
        .collect();

    Ok(Json(result))
}

async fn create_system_notification(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateSystemNotificationRequest>,
) -> Result<impl IntoResponse, AppError> {
    let title = req.title.trim();
    if title.is_empty() || title.len() > 160 {
        return Err(AppError::BadRequest(
            "Notification title must be between 1 and 160 characters".into(),
        ));
    }
    let body = req
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if body.is_some_and(|value| value.len() > 1_000) {
        return Err(AppError::BadRequest(
            "Notification body must be at most 1000 characters".into(),
        ));
    }

    let channel = notification_channel(&state.db, &auth.claims.sub, "system")
        .await
        .map_err(AppError::internal)?;
    if !notification_delivery_plan(channel.as_deref().or(Some("in_app"))).in_app {
        return Ok((StatusCode::OK, Json(None::<NotificationResponse>)));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO notifications (id, user_id, type, title, body, created_at) VALUES ($1, $2, 'system', $3, $4, $5)",
    )
    .bind(&id)
    .bind(&auth.claims.sub)
    .bind(title)
    .bind(body)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok((
        StatusCode::CREATED,
        Json(Some(NotificationResponse {
            id,
            notification_type: "system".to_string(),
            title: title.to_string(),
            body: body.map(str::to_string),
            source_user: None,
            target_type: None,
            target_id: None,
            read: false,
            created_at,
        })),
    ))
}

async fn update_notification_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<UpdateNotificationPreferencesRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_notification_preference_updates(&req.preferences)?;

    for preference in req.preferences {
        sqlx::query(
            "INSERT INTO notification_prefs (id, user_id, notification_type, channel) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT(user_id, notification_type) DO UPDATE SET channel = excluded.channel",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&auth.claims.sub)
        .bind(preference.notification_type)
        .bind(preference.channel)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    }

    get_notification_preferences(State(state), auth).await
}

/// Helper to create a notification (called from other modules)
pub async fn notify(
    db: &sqlx::AnyPool,
    user_id: &str,
    notification_type: &str,
    title: &str,
    body: Option<&str>,
    source_user_id: Option<&str>,
    target_type: Option<&str>,
    target_id: Option<&str>,
) {
    if !current_page_delivery_visible(db, user_id, target_type, target_id).await {
        return;
    }
    let channel = match notification_channel(db, user_id, notification_type).await {
        Ok(channel) => channel,
        Err(error) => {
            tracing::warn!(
                "Failed to read notification preferences for {}: {}",
                user_id,
                error
            );
            None
        }
    };

    let delivery_plan = notification_delivery_plan(channel.as_deref());
    if !delivery_plan.in_app && !delivery_plan.email {
        return;
    }

    let id = uuid::Uuid::new_v4().to_string();
    if delivery_plan.in_app {
        if !current_page_delivery_visible(db, user_id, target_type, target_id).await {
            return;
        }
        let inserted = sqlx::query(
            "INSERT INTO notifications (id, user_id, type, title, body, source_user_id, target_type, target_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&id)
        .bind(user_id)
        .bind(notification_type)
        .bind(title)
        .bind(body)
        .bind(source_user_id)
        .bind(target_type)
        .bind(target_id)
        .execute(db)
        .await;

        if let Err(error) = inserted {
            tracing::warn!("Failed to insert notification for {}: {}", user_id, error);
            return;
        }
    }

    if delivery_plan.email {
        if !current_page_delivery_visible(db, user_id, target_type, target_id).await {
            return;
        }
        if let Err(error) = send_email_notification(
            db,
            user_id,
            notification_type,
            title,
            body,
            target_type,
            target_id,
        )
        .await
        {
            tracing::warn!(
                "Failed to send notification email to {}: {}",
                user_id,
                error
            );
        }
    }
}

#[cfg(test)]
thread_local! {
    static BEFORE_EMAIL_DELIVERY_CHECK: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>>> = std::cell::RefCell::new(None);
}

async fn send_email_notification(
    db: &sqlx::AnyPool,
    user_id: &str,
    notification_type: &str,
    title: &str,
    body: Option<&str>,
    target_type: Option<&str>,
    target_id: Option<&str>,
) -> Result<(), anyhow::Error> {
    let smtp_host = config_value(db, "smtp_host", "SMTP_HOST", "").await?;
    if smtp_host.trim().is_empty() {
        return Ok(());
    }

    let channel = notification_channel(db, user_id, notification_type).await?;
    if !notification_delivery_plan(channel.as_deref()).email {
        return Ok(());
    }

    let recipient: Option<(String,)> =
        sqlx::query_as("SELECT email FROM users WHERE id = $1 AND is_active = 1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    let Some((recipient_email,)) = recipient else {
        return Ok(());
    };

    let smtp_port = config_value(db, "smtp_port", "SMTP_PORT", "587")
        .await?
        .parse::<u16>()
        .unwrap_or(587);
    let smtp_user = config_value(db, "smtp_user", "SMTP_USER", "").await?;
    let smtp_password = config_value(db, "smtp_password", "SMTP_PASSWORD", "").await?;
    let smtp_from = config_value(db, "smtp_from", "SMTP_FROM", "").await?;
    let from_address = if smtp_from.trim().is_empty() {
        smtp_user.trim()
    } else {
        smtp_from.trim()
    };
    if from_address.is_empty() {
        return Ok(());
    }

    let email = Message::builder()
        .from(from_address.parse::<Mailbox>()?)
        .to(recipient_email.parse::<Mailbox>()?)
        .subject(title)
        .body(body.unwrap_or(title).to_string())?;

    let mut transport_builder =
        AsyncSmtpTransport::<Tokio1Executor>::relay(smtp_host.trim())?.port(smtp_port);
    if !smtp_user.trim().is_empty() {
        transport_builder =
            transport_builder.credentials(Credentials::new(smtp_user, smtp_password));
    }

    #[cfg(test)]
    {
        let hook = BEFORE_EMAIL_DELIVERY_CHECK.with(|slot| slot.borrow_mut().take());
        if let Some(hook) = hook {
            hook.await;
        }
    }
    // SMTP configuration/recipient reads can wait. Recheck page visibility after
    // those waits, immediately before delivery; transport itself is external.
    if !current_page_delivery_visible(db, user_id, target_type, target_id).await {
        return Ok(());
    }
    transport_builder.build().send(email).await?;
    Ok(())
}

#[cfg(test)]
tokio::task_local! {
    pub(crate) static RESET_EMAIL_CAPTURE: std::cell::RefCell<Option<Box<dyn FnMut(&str, &str, &str) -> std::pin::Pin<Box<dyn std::future::Future<Output=Result<bool, anyhow::Error>> + Send>> + Send>>>;
}

pub async fn send_direct_email(
    db: &sqlx::AnyPool,
    recipient_email: &str,
    subject: &str,
    body: &str,
) -> Result<bool, anyhow::Error> {
    #[cfg(test)]
    if let Ok(Some(result)) = RESET_EMAIL_CAPTURE.try_with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .map(|capture| capture(recipient_email, subject, body))
    }) {
        return result.await;
    }
    let smtp_host = config_value(db, "smtp_host", "SMTP_HOST", "").await?;
    if smtp_host.trim().is_empty() {
        return Ok(false);
    }

    let smtp_port = config_value(db, "smtp_port", "SMTP_PORT", "587")
        .await?
        .parse::<u16>()
        .unwrap_or(587);
    let smtp_user = config_value(db, "smtp_user", "SMTP_USER", "").await?;
    let smtp_password = config_value(db, "smtp_password", "SMTP_PASSWORD", "").await?;
    let smtp_from = config_value(db, "smtp_from", "SMTP_FROM", "").await?;
    let from_address = if smtp_from.trim().is_empty() {
        smtp_user.trim()
    } else {
        smtp_from.trim()
    };
    if from_address.is_empty() {
        return Ok(false);
    }

    let email = Message::builder()
        .from(from_address.parse::<Mailbox>()?)
        .to(recipient_email.parse::<Mailbox>()?)
        .subject(subject)
        .body(body.to_string())?;

    let mut transport_builder =
        AsyncSmtpTransport::<Tokio1Executor>::relay(smtp_host.trim())?.port(smtp_port);
    if !smtp_user.trim().is_empty() {
        transport_builder =
            transport_builder.credentials(Credentials::new(smtp_user, smtp_password));
    }

    transport_builder.build().send(email).await?;
    Ok(true)
}

async fn notification_channel(
    db: &sqlx::AnyPool,
    user_id: &str,
    notification_type: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_as(
        "SELECT channel FROM notification_prefs WHERE user_id = $1 AND notification_type = $2",
    )
    .bind(user_id)
    .bind(notification_type)
    .fetch_optional(db)
    .await
    .map(|row: Option<(String,)>| row.map(|item| item.0))
}

async fn config_value(
    db: &sqlx::AnyPool,
    key: &str,
    env_var: &str,
    default: &str,
) -> Result<String, sqlx::Error> {
    if let Ok(value) = std::env::var(env_var) {
        return Ok(value);
    }

    sqlx::query_as("SELECT value FROM system_config WHERE key = $1")
        .bind(key)
        .fetch_optional(db)
        .await
        .map(|row: Option<(String,)>| {
            row.map(|item| item.0)
                .unwrap_or_else(|| default.to_string())
        })
}

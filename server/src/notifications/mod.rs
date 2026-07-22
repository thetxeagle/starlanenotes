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

async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<NotificationListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let (before_created_at, before_id) = notification_cursor_parts(query.before.as_deref())?;
    let unread_only = i32::from(query.unread_only.unwrap_or(false));
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
             WHERE n.user_id = ?1 \
               AND (?2 = 0 OR n.read = 0) \
               AND (?3 IS NULL OR n.created_at < ?3 OR (n.created_at = ?3 AND n.id < ?4)) \
             ORDER BY n.created_at DESC, n.id DESC LIMIT ?5",
    )
    .bind(&auth.claims.sub)
    .bind(unread_only)
    .bind(before_created_at)
    .bind(before_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<NotificationResponse> = notifications
        .into_iter()
        .map(
            |(id, ntype, title, body, source, ttype, tid, read, created)| NotificationResponse {
                id,
                notification_type: ntype,
                title,
                body,
                source_user: source,
                target_type: ttype,
                target_id: tid,
                read: read != 0,
                created_at: created,
            },
        )
        .collect();

    Ok(Json(result))
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
}

async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(notification_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let read_at = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE notifications SET read = 1, read_at = ?1 WHERE id = ?2 AND user_id = ?3",
    )
    .bind(&read_at)
    .bind(&notification_id)
    .bind(&auth.claims.sub)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Notification not found".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn mark_all_read(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let read_at = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE notifications SET read = 1, read_at = ?1 WHERE user_id = ?2 AND read = 0")
        .bind(&read_at)
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_notification_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT notification_type, channel FROM notification_prefs WHERE user_id = ?1",
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
    let body = req.body.as_deref().map(str::trim).filter(|value| !value.is_empty());
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
        "INSERT INTO notifications (id, user_id, type, title, body, created_at) VALUES (?1, ?2, 'system', ?3, ?4, ?5)",
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
             VALUES (?1, ?2, ?3, ?4) \
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
        let inserted = sqlx::query(
            "INSERT INTO notifications (id, user_id, type, title, body, source_user_id, target_type, target_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
        if let Err(error) =
            send_email_notification(db, user_id, notification_type, title, body).await
        {
            tracing::warn!(
                "Failed to send notification email to {}: {}",
                user_id,
                error
            );
        }
    }
}

async fn send_email_notification(
    db: &sqlx::AnyPool,
    user_id: &str,
    notification_type: &str,
    title: &str,
    body: Option<&str>,
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
        sqlx::query_as("SELECT email FROM users WHERE id = ?1 AND is_active = 1")
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

    transport_builder.build().send(email).await?;
    Ok(())
}

pub async fn send_direct_email(
    db: &sqlx::AnyPool,
    recipient_email: &str,
    subject: &str,
    body: &str,
) -> Result<bool, anyhow::Error> {
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
        "SELECT channel FROM notification_prefs WHERE user_id = ?1 AND notification_type = ?2",
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

    sqlx::query_as("SELECT value FROM system_config WHERE key = ?1")
        .bind(key)
        .fetch_optional(db)
        .await
        .map(|row: Option<(String,)>| {
            row.map(|item| item.0)
                .unwrap_or_else(|| default.to_string())
        })
}

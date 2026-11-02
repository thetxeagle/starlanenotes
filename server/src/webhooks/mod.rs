pub mod dispatch;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::{require_current_admin, AdminUser};
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/webhooks", get(list_webhooks).post(create_webhook))
        .route(
            "/webhooks/{webhook_id}",
            get(get_webhook).put(update_webhook).delete(delete_webhook),
        )
        .route(
            "/webhooks/{webhook_id}/test",
            axum::routing::post(test_webhook),
        )
        .route("/webhooks/{webhook_id}/deliveries", get(list_deliveries))
        .route(
            "/webhooks/{webhook_id}/deliveries/{delivery_id}/redeliver",
            axum::routing::post(redeliver),
        )
}

#[derive(Deserialize)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub url: String,
    pub secret: Option<String>,
    pub events: Vec<String>,
    pub space_id: Option<String>,
}

#[derive(Serialize)]
pub struct WebhookResponse {
    pub id: String,
    pub name: String,
    pub url: String,
    pub events: Vec<String>,
    pub space_id: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    pub created_by: String,
}

#[derive(Serialize)]
pub struct DeliveryResponse {
    pub id: String,
    pub event_type: String,
    pub response_status: Option<i32>,
    pub success: bool,
    pub attempt_number: i32,
    pub delivered_at: String,
    pub test_id: Option<String>,
}

async fn list_webhooks(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let webhooks: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        i32,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, name, url, events, space_id, is_active, created_at, created_by \
             FROM webhooks ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<WebhookResponse> = webhooks
        .into_iter()
        .map(
            |(id, name, url, events, space_id, active, created, created_by)| WebhookResponse {
                id,
                name,
                url,
                events: serde_json::from_str(&events).unwrap_or_default(),
                space_id,
                is_active: active != 0,
                created_at: created,
                created_by,
            },
        )
        .collect();

    Ok(Json(result))
}

async fn create_webhook(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_webhook_request(&req)?;
    let webhook_id = Uuid::new_v4().to_string();
    let events_json =
        serde_json::to_string(&req.events).map_err(|e| AppError::Internal(e.to_string()))?;

    let secret_hash = req.secret.as_deref().map(hash_webhook_secret);

    sqlx::query(
        "INSERT INTO webhooks (id, space_id, name, url, secret_hash, events, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&webhook_id)
    .bind(&req.space_id)
    .bind(&req.name)
    .bind(&req.url)
    .bind(&secret_hash)
    .bind(&events_json)
    .bind(&admin.claims.sub)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "webhook.create",
        Some("webhook"),
        Some(&webhook_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": webhook_id })),
    ))
}

async fn get_webhook(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(webhook_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let wh: Option<(String, String, String, String, Option<String>, i32, String, String)> = sqlx::query_as(
        "SELECT id, name, url, events, space_id, is_active, created_at, created_by FROM webhooks WHERE id = $1",
    )
    .bind(&webhook_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (id, name, url, events, space_id, active, created, created_by) =
        wh.ok_or_else(|| AppError::NotFound("Webhook not found".into()))?;

    Ok(Json(WebhookResponse {
        id,
        name,
        url,
        events: serde_json::from_str(&events).unwrap_or_default(),
        space_id,
        is_active: active != 0,
        created_at: created,
        created_by,
    }))
}

async fn update_webhook(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(webhook_id): Path<String>,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_webhook_request(&req)?;
    let events_json =
        serde_json::to_string(&req.events).map_err(|e| AppError::Internal(e.to_string()))?;
    let secret_hash = req.secret.as_deref().map(hash_webhook_secret);

    let result = sqlx::query(
        "UPDATE webhooks SET name = $1, url = $2, events = $3, \
         secret_hash = COALESCE($4, secret_hash), updated_at = $5 WHERE id = $6",
    )
    .bind(&req.name)
    .bind(&req.url)
    .bind(&events_json)
    .bind(&secret_hash)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&webhook_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Webhook not found".into()));
    }

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "webhook.update",
        Some("webhook"),
        Some(&webhook_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

fn hash_webhook_secret(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

fn validate_webhook_request(req: &CreateWebhookRequest) -> Result<(), AppError> {
    if req.name.trim().is_empty() || req.name.trim().len() > 120 {
        return Err(AppError::BadRequest(
            "Webhook name must be 1 to 120 characters".into(),
        ));
    }
    let url = reqwest::Url::parse(req.url.trim())
        .map_err(|_| AppError::BadRequest("Webhook URL must be a valid HTTP(S) URL".into()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(AppError::BadRequest(
            "Webhook URL must be a valid HTTP(S) URL".into(),
        ));
    }
    if req.events.is_empty()
        || req.events.len() > 50
        || req.events.iter().any(|event| event.trim().is_empty())
    {
        return Err(AppError::BadRequest(
            "Webhook events must contain 1 to 50 non-empty entries".into(),
        ));
    }
    if req.events.iter().any(|event| event != dispatch::PUBLIC_PAGE_PUBLISHED_EVENT) {
        return Err(AppError::BadRequest(
            "Ordinary webhooks support only public_page.published; private and wildcard exports require a separate grant design".into(),
        ));
    }
    Ok(())
}

async fn delete_webhook(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(webhook_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    require_current_admin(&mut tx, &admin).await?;
    let result = sqlx::query("DELETE FROM webhooks WHERE id = $1")
        .bind(&webhook_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Webhook not found".into()));
    }

    require_current_admin(&mut tx, &admin).await?;
    tx.commit().await.map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "webhook.delete",
        Some("webhook"),
        Some(&webhook_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_deliveries(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(webhook_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let deliveries: Vec<(String, String, Option<i32>, i32, i32, String, String)> = sqlx::query_as(
        "SELECT id, event_type, response_status, success, attempt_number, delivered_at, payload \
         FROM webhook_deliveries WHERE webhook_id = $1 \
         ORDER BY delivered_at DESC LIMIT 50",
    )
    .bind(&webhook_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<DeliveryResponse> = deliveries
        .into_iter()
        .map(
            |(id, event, status, success, attempts, delivered, payload)| DeliveryResponse {
                test_id: delivery_test_id(&event, &payload),
                id,
                event_type: event,
                response_status: status,
                success: success != 0,
                attempt_number: attempts,
                delivered_at: delivered,
            },
        )
        .collect();

    Ok(Json(result))
}

fn delivery_test_id(event: &str, payload: &str) -> Option<String> {
    if event != "webhook.test" {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    value
        .get("test_id")?
        .as_str()?
        .parse::<Uuid>()
        .ok()
        .map(|id| id.to_string())
}

async fn test_webhook(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(webhook_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let active: Option<i32> = sqlx::query_scalar("SELECT is_active FROM webhooks WHERE id = $1")
        .bind(&webhook_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    require_current_admin(&mut conn, &admin).await?;
    match active {
        None => return Err(AppError::NotFound("Webhook not found".into())),
        Some(0) => return Err(AppError::BadRequest("Webhook is inactive".into())),
        _ => {}
    }
    let delivery_id = dispatch::queue_test_delivery(&state.db, &webhook_id);
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"status": "queued", "delivery_id": delivery_id})),
    ))
}

async fn redeliver(
    State(state): State<AppState>,
    admin: AdminUser,
    Path((webhook_id, delivery_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let queued =
        dispatch::redeliver_recorded_delivery(&state.db, &webhook_id, &delivery_id, &admin).await?;
    if !queued {
        return Err(AppError::NotFound("Webhook delivery not found".into()));
    }

    Ok(StatusCode::ACCEPTED)
}

#[cfg(test)]
mod tests {
    use super::{validate_webhook_request, CreateWebhookRequest};

    fn request(url: &str, events: Vec<&str>) -> CreateWebhookRequest {
        CreateWebhookRequest {
            name: "Document updates".to_string(),
            url: url.to_string(),
            secret: None,
            events: events.into_iter().map(str::to_string).collect(),
            space_id: None,
        }
    }

    #[test]
    fn webhook_requests_require_a_valid_destination_and_events() {
        assert!(validate_webhook_request(&request(
            "https://hooks.example.test/events",
            vec!["public_page.published"]
        ))
        .is_ok());
        assert!(validate_webhook_request(&request(
            "mailto:ops@example.test",
            vec!["document.updated"]
        ))
        .is_err());
        assert!(
            validate_webhook_request(&request("https://hooks.example.test/events", vec![]))
                .is_err()
        );
        assert!(
            validate_webhook_request(&request("https://hooks.example.test/events", vec![" "]))
                .is_err()
        );
        assert!(validate_webhook_request(&request(
            "https://hooks.example.test/events",
            vec!["*"]
        )).is_err());
    }
}

#[cfg(test)]
mod integration_tests;

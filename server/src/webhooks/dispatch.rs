use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;
use sqlx::AnyPool;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;
const WEBHOOK_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const MAX_RESPONSE_BODY_BYTES: usize = 16 * 1024;

#[derive(Clone)]
enum DeliverySelection {
    PublicPage { space_id: String, document_id: String },
    Explicit,
}

pub const PUBLIC_PAGE_PUBLISHED_EVENT: &str = "public_page.published";

/// Queue matching events; each attempt rechecks current configuration before sending.
pub async fn dispatch_event(
    db: &AnyPool,
    event_type: &str,
    payload: &serde_json::Value,
    space_id: Option<&str>,
) {
    // Retain legacy call sites without letting unbound private resource events
    // cross the ordinary integration boundary.
    if event_type != PUBLIC_PAGE_PUBLISHED_EVENT { return; }
    let Some(space_id) = space_id else { return; };
    let Some(document_id) = payload.get("document_id").and_then(serde_json::Value::as_str) else { return; };
    dispatch_public_page_published(db, space_id, document_id).await;
}

pub async fn dispatch_public_page_published(db: &AnyPool, space_id: &str, document_id: &str) {
    let Some(payload) = current_public_page_payload(db, space_id, document_id).await else { return; };
    let webhooks = matching_webhooks(db, Some(space_id)).await.unwrap_or_default();
    let payload = serde_json::to_string(&payload).unwrap_or_default();
    for (id, _url, _secret_hash, events) in webhooks {
        if !webhook_event_matches(
            &serde_json::from_str::<Vec<String>>(&events).unwrap_or_default(),
            PUBLIC_PAGE_PUBLISHED_EVENT,
        ) {
            continue;
        }
        queue_delivery(
            db,
            &id,
            PUBLIC_PAGE_PUBLISHED_EVENT,
            &payload,
            DeliverySelection::PublicPage {
                space_id: space_id.to_owned(),
                document_id: document_id.to_owned(),
            },
            None,
        );
    }
}

async fn current_public_page_payload(db: &AnyPool, space_id: &str, document_id: &str) -> Option<serde_json::Value> {
    let row: Option<(String, String)> = sqlx::query_as("SELECT title,published_at FROM documents WHERE id=$1 AND space_id=$2 AND public=1 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(document_id).bind(space_id).fetch_optional(db).await.ok()?;
    let (title, published_at) = row?;
    Some(serde_json::json!({"schema_version":1,"event":PUBLIC_PAGE_PUBLISHED_EVENT,"space_id":space_id,"document_id":document_id,"title":title,"published_at":published_at}))
}

/// Read the existing active scoped/global candidates; delivery and subscription policy stay at dispatch.
async fn matching_webhooks(db: &AnyPool, space_id: Option<&str>)
    -> Result<Vec<(String, String, Option<String>, String)>, sqlx::Error>
{
    if let Some(sid) = space_id {
        sqlx::query_as(
            "SELECT id, url, secret_hash, events FROM webhooks \
             WHERE is_active = 1 AND (space_id = $1 OR space_id IS NULL)",
        )
        .bind(sid)
        .fetch_all(db)
        .await
    } else {
        sqlx::query_as(
            "SELECT id, url, secret_hash, events FROM webhooks \
             WHERE is_active = 1 AND space_id IS NULL",
        )
        .fetch_all(db)
        .await
    }
}

fn queue_delivery(
    db: &AnyPool,
    id: &str,
    event: &str,
    payload: &str,
    selection: DeliverySelection,
    initial_delivery_id: Option<String>,
) {
    let (db, id, event, payload) = (
        db.clone(),
        id.to_owned(),
        event.to_owned(),
        payload.to_owned(),
    );
    tokio::spawn(async move {
        deliver_webhook(&db, &id, &event, &payload, selection, initial_delivery_id).await;
    });
}

/// Caller verifies existence/current authority before queueing this neutral test.
pub(super) fn queue_test_delivery(db: &AnyPool, id: &str) -> String {
    let delivery_id = Uuid::new_v4().to_string();
    let payload =
        serde_json::json!({"event":"webhook.test","test":true,"test_id":delivery_id}).to_string();
    queue_delivery(
        db,
        id,
        "webhook.test",
        &payload,
        DeliverySelection::Explicit,
        Some(delivery_id.clone()),
    );
    delivery_id
}

async fn deliver_webhook(
    db: &AnyPool,
    webhook_id: &str,
    event_type: &str,
    payload: &str,
    selection: DeliverySelection,
    initial_delivery_id: Option<String>,
) {
    let client = Client::builder()
        .timeout(WEBHOOK_REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("static webhook client configuration is valid");
    for attempt in 1..=3 {
        // Disconnect cannot cancel an already in-flight request. It prevents subsequent sends.
        let config: Option<(String, Option<String>, String, Option<String>)> = match sqlx::query_as(
            "SELECT url, secret_hash, events, space_id FROM webhooks WHERE id = $1 AND is_active = 1"
        ).bind(webhook_id).fetch_optional(db).await {
            Ok(value) => value,
            Err(_) => { tracing::warn!("Webhook configuration unavailable; delivery stopped"); return; }
        };
        let Some((url, secret_hash, events, configured_space)) = config else {
            return;
        };
        if let DeliverySelection::PublicPage { space_id, document_id } = &selection {
            if configured_space.is_some() && configured_space.as_ref() != Some(space_id) {
                return;
            }
            if !webhook_event_matches(
                &serde_json::from_str::<Vec<String>>(&events).unwrap_or_default(),
                event_type,
            ) {
                return;
            }
            if event_type != PUBLIC_PAGE_PUBLISHED_EVENT
                || current_public_page_payload(db, space_id, document_id).await.is_none()
            {
                return;
            }
        }
        let delivery_id = if attempt == 1 {
            initial_delivery_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string())
        } else {
            Uuid::new_v4().to_string()
        };
        let mut request = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Starlane-Event", event_type)
            .header("X-Starlane-Delivery", &delivery_id);
        // Preserve the existing lowercase hash-hex UTF-8 HMAC key contract.
        if let Some(hash) = secret_hash {
            if let Ok(mut mac) = HmacSha256::new_from_slice(hash.as_bytes()) {
                mac.update(payload.as_bytes());
                request = request.header(
                    "X-Starlane-Signature",
                    format!("sha256={}", hex::encode(mac.finalize().into_bytes())),
                );
            }
        }
        let (status, success, response_body) = match request.body(payload.to_owned()).send().await {
            Ok(mut response) => {
                let status = i32::from(response.status().as_u16());
                let success = response.status().is_success();
                let mut captured = Vec::new();
                while captured.len() < MAX_RESPONSE_BODY_BYTES {
                    match response.chunk().await {
                        Ok(Some(chunk)) => {
                            let remaining = MAX_RESPONSE_BODY_BYTES - captured.len();
                            captured.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                        }
                        _ => break,
                    }
                }
                let mut body = String::from_utf8_lossy(&captured).into_owned();
                if body.len() > MAX_RESPONSE_BODY_BYTES {
                    let mut boundary = MAX_RESPONSE_BODY_BYTES;
                    while !body.is_char_boundary(boundary) {
                        boundary -= 1;
                    }
                    body.truncate(boundary);
                }
                (Some(status), success, body)
            }
            Err(error) => {
                // reqwest errors may contain endpoint credentials/query strings.
                tracing::warn!("Webhook delivery request failed");
                (None, false, error.without_url().to_string())
            }
        };
        let _ = sqlx::query("INSERT INTO webhook_deliveries (id, webhook_id, event_type, payload, response_status, response_body, attempt_number, success) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)")
            .bind(&delivery_id).bind(webhook_id).bind(event_type).bind(payload).bind(status)
            .bind(response_body).bind(attempt).bind(i64::from(success)).execute(db).await;
        if success || attempt == 3 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt as u32))).await;
    }
}

fn webhook_event_matches(events: &[String], event_type: &str) -> bool {
    events.iter().any(|event| event == event_type)
}

pub async fn redeliver_recorded_delivery(
    db: &AnyPool,
    webhook_id: &str,
    delivery_id: &str,
    admin: &crate::auth::middleware::AdminUser,
) -> Result<bool, crate::auth::routes::AppError> {
    let mut conn = db
        .acquire()
        .await
        .map_err(crate::auth::routes::AppError::internal)?;
    let delivery: Option<(String, String)> = sqlx::query_as(
        "SELECT d.event_type, d.payload FROM webhook_deliveries d INNER JOIN webhooks w ON w.id = d.webhook_id WHERE d.webhook_id = $1 AND d.id = $2 AND w.is_active = 1"
    ).bind(webhook_id).bind(delivery_id).fetch_optional(&mut *conn).await.map_err(crate::auth::routes::AppError::internal)?;
    crate::auth::middleware::require_current_admin(&mut conn, admin).await?;
    let Some((event, payload)) = delivery else {
        return Ok(false);
    };
    drop(conn);
    let selection = if event == "webhook.test" {
        DeliverySelection::Explicit
    } else if event == PUBLIC_PAGE_PUBLISHED_EVENT {
        let value: serde_json::Value = serde_json::from_str(&payload).map_err(|_| {
            crate::auth::routes::AppError::Conflict("Stored webhook payload is invalid".into())
        })?;
        let space_id = value
            .get("space_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                crate::auth::routes::AppError::Conflict(
                    "Stored webhook payload is missing its space".into(),
                )
            })?;
        let document_id = value
            .get("document_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                crate::auth::routes::AppError::Conflict(
                    "Stored webhook payload is missing its page".into(),
                )
            })?;
        if current_public_page_payload(db, space_id, document_id)
            .await
            .is_none()
        {
            return Ok(false);
        }
        DeliverySelection::PublicPage {
            space_id: space_id.into(),
            document_id: document_id.into(),
        }
    } else {
        return Ok(false);
    };
    queue_delivery(
        db,
        webhook_id,
        &event,
        &payload,
        selection,
        None,
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::webhook_event_matches;
    #[test]
    fn webhook_event_matching_requires_an_exact_subscription() {
        assert!(webhook_event_matches(
            &["document.updated".into()],
            "document.updated"
        ));
        assert!(!webhook_event_matches(&["*".into()], "space.created"));
        assert!(!webhook_event_matches(
            &["document.updated".into()],
            "document.deleted"
        ));
        assert!(!webhook_event_matches(
            &[" document.updated ".into()],
            "document.updated"
        ));
    }
}

#[cfg(test)]
#[path = "postgres_audit_webhook_fixture.rs"]
mod postgres_audit_webhook_fixture;
